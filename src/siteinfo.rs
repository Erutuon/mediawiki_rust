#![allow(unused)]
use crate::traits::Errorable;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    convert::TryFrom,
    fmt::Display,
    str::FromStr, borrow::Cow,
};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SiteInfo {
    #[serde(rename = "batchcomplete")]
    pub batch_complete: bool,
    pub query: Option<SiteInfoQuery>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl SiteInfo {
    pub fn namespaces(&self) -> Option<&BTreeMap<NamespaceId, NamespaceInfo>> {
        self.query.as_ref().map(|q| q.namespaces.as_ref()).flatten()
    }

    pub fn namespace_aliases(&self) -> Option<&[NamespaceAlias]> {
        self.query
            .as_ref()
            .map(|q| q.namespace_aliases.as_deref())
            .flatten()
    }

    pub fn namespace_info_by_id(
        &self,
        id: NamespaceId,
    ) -> Option<&NamespaceInfo> {
        self.namespaces().map(|n| n.get(&id)).flatten()
    }

    pub fn namespace_info_by_name<'a>(
        &'a self,
        name: &str,
    ) -> Option<&'a NamespaceInfo> {
        self.namespaces()
            .map(|namespaces| {
                namespaces
                    .values()
                    .find(|namespace_info| {
                        namespace_info.name == name
                            || namespace_info.canonical.as_deref() == Some(name)
                    })
                    .or_else(|| {
                        self.namespace_aliases()
                            .map(|aliases| {
                                aliases
                                    .iter()
                                    .find(|alias| alias.alias == name)
                                    .map(|alias| namespaces.get(&alias.id))
                                    .flatten()
                            })
                            .flatten()
                    })
            })
            .flatten()
    }

    pub fn get_general_info(&self, key: &str) -> Option<&Value> {
        self.query
            .as_ref()
            .map(|q| {
                q.general.as_ref().map(|g| g.extra.get(key))
            })
            .flatten()
            .flatten()
    }
}

trait GetLag {
    fn get_lag(&self) -> Option<u64>;
}

impl GetLag for SiteInfo {
    fn get_lag(&self) -> Option<u64> {
        self.extra
            .get("error")
            .map(|e| {
                if e["code"] == "maxlag" {
                    e["lag"].as_u64()
                } else {
                    None
                }
            })
            .flatten()
    }
}

impl Errorable for SiteInfo {
    fn get_error(&self) -> Option<&Map<String, Value>> {
        self.extra.get("error").map(|e| e.as_object()).flatten()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SiteInfoQuery {
    pub general: Option<GeneralSiteInfo>,
    pub namespaces: Option<BTreeMap<NamespaceId, NamespaceInfo>>,
    pub namespace_aliases: Option<Vec<NamespaceAlias>>,
    pub libraries: Option<Vec<LibraryInfo>>,
    pub extensions: Option<Vec<ExtensionInfo>>,
    pub statistics: Option<Statistics>,
}

/// Alias for a namespace (could be -1 for Special pages etc.)
pub type NamespaceId = i32;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct GeneralSiteInfo {
    #[serde(rename = "mainpage")]
    pub main_page: String,
    pub base: String,
    #[serde(rename = "sitename")]
    pub site_name: String,
    #[serde(rename = "mainpageisdomainroot")]
    pub main_page_is_domain_root: bool,
    pub logo: String,
    pub generator: String,
    #[serde(rename = "phpversion")]
    pub php_version: String,
    #[serde(rename = "phpsapi")]
    pub php_server_api_version: String,
    #[serde(rename = "dbtype")]
    pub database_type: String,
    #[serde(rename = "dbversion")]
    pub database_version: String,
    #[serde(rename = "imagewhitelistenabled")]
    pub image_whitelist_enabled: bool,
    #[serde(rename = "langconversion")]
    pub lang_conversion: bool,
    #[serde(rename = "titleconversion")]
    pub title_conversion: bool,
    #[serde(rename = "linkprefixcharset")]
    pub title_prefix_char_set: String,
    #[serde(rename = "linkprefix")]
    pub link_prefix: String,
    #[serde(rename = "linktrail")]
    pub link_trail: String,
    #[serde(rename = "legaltitlechars")]
    pub legal_title_chars: String,
    #[serde(rename = "invalidusernamechars")]
    pub invalid_username_chars: String,
    #[serde(rename = "allunicodefixes")]
    pub all_unicode_fixes: bool,
    #[serde(rename = "fixarabicunicode")]
    pub fix_arabic_unicode: bool,
    #[serde(rename = "fixmalayalamunicode")]
    pub fix_malayalam_unicode: bool,
    #[serde(rename = "git-hash")]
    pub git_hash: String,
    #[serde(rename = "git-branch")]
    pub git_branch: String,
    pub case: CaseSensitivity,
    pub lang: String,
    pub fallback: Vec<Value>,
    pub rtl: bool,
    #[serde(rename = "fallback8bitEncoding")]
    pub fallback_eight_bit_encoding: String,
    #[serde(rename = "readonly")]
    pub read_only: bool,
    #[serde(rename = "writeapi")]
    pub write_api: bool,
    #[serde(rename = "maxarticlesize")]
    pub max_article_size: u64,
    #[serde(rename = "timezone")]
    pub time_zone: String,
    #[serde(rename = "timeoffset")]
    pub time_offset: u8,
    #[serde(rename = "articlepath")]
    pub article_path: String,
    #[serde(rename = "scriptpath")]
    pub script_path: String,
    pub script: String,
    #[serde(rename = "variantarticlepath")]
    pub variant_article_path: bool,
    pub server: String,
    #[serde(rename = "servername")]
    pub server_name: String,
    #[serde(rename = "wikiid")]
    pub wiki_id: String,
    pub time: String, // todo: use type for time and date with timezone
    #[serde(rename = "misermode")]
    pub miser_mode: bool,
    #[serde(rename = "uploadsenabled")]
    pub uploads_enabled: bool,
    #[serde(rename = "maxuploadsize")]
    pub max_upload_size: u64,
    #[serde(rename = "minuploadchunksize")]
    pub min_upload_chunk_size: u32,
    #[serde(rename = "galleryoptions")]
    pub gallery_options: GalleryOptions,
    #[serde(rename = "thumblimits")]
    pub thumb_limits: MapVec<ThumbLimit>,
    #[serde(rename = "imagelimits")]
    pub image_limits: MapVec<ImageDimensions>,
    pub favicon: String,
    #[serde(rename = "centralidlookupprovider")]
    pub central_id_lookup_provider: String,
    #[serde(rename = "allcentralidlookupproviders")]
    pub all_central_id_lookup_providers: Vec<String>,
    #[serde(rename = "interwikimagic")]
    pub interwiki_magic: bool,
    #[serde(rename = "magiclinks")]
    pub magic_links: MapSet,
    #[serde(rename = "categorycollation")]
    pub category_collation: String,
    #[serde(rename = "wmf-config")]
    pub wmf_config: Value,
    #[serde(rename = "citeresponsivereferences")]
    pub cite_responsive_references: bool,
    pub linter: BTreeMap<String, Vec<String>>,
    #[serde(rename = "mobileserver")]
    pub mobile_server: String,
    #[serde(rename = "pageviewservice-supported-metrics")]
    pub page_view_service_supported_metrics:
        BTreeMap<String, BTreeMap<String, bool>>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize)]
pub struct GalleryOptions {
    #[serde(rename = "imagesPerRow")]
    pub images_per_row: u8,
    #[serde(rename = "imageWidth")]
    pub image_width: u8,
    #[serde(rename = "imageHeight")]
    pub image_height: u8,
    #[serde(rename = "captionLength")]
    pub caption_length: bool,
    #[serde(rename = "showBytes")]
    pub show_bytes: bool,
    pub mode: String,
    #[serde(rename = "showDimensions")]
    pub show_dimensions: bool,
}

pub type ThumbLimit = u32;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize)]
#[serde(try_from = "BTreeMap<&str, T>", into = "BTreeMap<String, T>")]
pub struct MapVec<T: Clone>(Vec<T>);

impl<T: Clone> TryFrom<BTreeMap<&str, T>> for MapVec<T> {
    type Error = Cow<'static, str>;
    fn try_from(m: BTreeMap<&str, T>) -> Result<Self, Self::Error> {
        let mut i = 0;
        let vec = m
            .into_iter()
            .map(|(k, v)| {
                let k = k.parse::<usize>().map_err(|_| "key not integer")?;
                if k != i {
                    Err(format!("key not sequential: {}, expected {}", k, i))
                } else {
                    i += 1;
                    Ok(v)
                }
            })
            .collect::<Result<_, _>>()?;
        Ok(Self(vec))
    }
}

impl<T: Clone> Into<BTreeMap<String, T>> for MapVec<T> {
    fn into(self) -> BTreeMap<String, T> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(k, v)| (k.to_string(), v))
            .collect()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize)]
#[serde(try_from = "BTreeMap<&str, bool>", into = "BTreeMap<String, bool>")]
pub struct MapSet(BTreeSet<String>);

impl Into<BTreeMap<String, bool>> for MapSet {
    fn into(self) -> BTreeMap<String, bool> {
        self.0.into_iter().map(|k| (k, true)).collect()
    }
}

impl TryFrom<BTreeMap<&str, bool>> for MapSet {
    type Error = &'static str;
    fn try_from(map: BTreeMap<&str, bool>) -> Result<Self, Self::Error> {
        let set = map
            .into_iter()
            .map(|(k, v)| {
                if v != true {
                    Err("value in BTreeMap was false")
                } else {
                    Ok(k.to_string())
                }
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        Ok(Self(set))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Deserialize, Serialize)]
pub struct ImageDimensions {
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(try_from = "&str", into = "&'static str")]
pub enum CaseSensitivity {
    FirstLetter,
    CaseSensitive,
}

impl From<CaseSensitivity> for &'static str {
    fn from(c: CaseSensitivity) -> &'static str {
        match c {
            CaseSensitivity::FirstLetter => "first-letter",
            CaseSensitivity::CaseSensitive => "case-sensitive",
        }
    }
}

impl TryFrom<&str> for CaseSensitivity {
    type Error = &'static str;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl FromStr for CaseSensitivity {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let val = match s {
            "first-letter" => Self::FirstLetter,
            "case-sensitive" => Self::CaseSensitive,
            _ => return Err("unrecognized value"),
        };
        return Ok(val);
    }
}

impl Display for CaseSensitivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", <&str>::from(*self))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Deserialize, Serialize)]
pub struct NamespaceInfo {
    pub id: NamespaceId,
    pub case: CaseSensitivity,
    pub name: String,
    pub subpages: bool,
    pub canonical: Option<String>,
    pub content: bool,
    pub nonincludable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Deserialize, Serialize)]
pub struct NamespaceAlias {
    pub id: NamespaceId,
    pub alias: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Deserialize, Serialize)]
pub struct LibraryInfo {
    pub name: String,
    pub version: String, // todo: use semver type?
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Deserialize, Serialize)]
pub struct Statistics {
    pages: usize,
    articles: usize,
    edits: usize,
    images: usize,
    users: usize,
    #[serde(rename = "activeusers")]
    active_users: usize,
    jobs: usize,
    #[serde(rename = "queued-massmessages")]
    queued_mass_messages: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Deserialize, Serialize)]
pub struct ExtensionInfo {
    r#type: String,
    name: String,
    #[serde(rename = "descriptionmsg")]
    description_msg: Option<String>,
    author: Option<String>,
    url: String, // todo: use URL type?
    #[serde(flatten)]
    version_control_system: VersionControlSystem,
    #[serde(flatten)]
    license: License,
    credits: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Deserialize, Serialize)]
pub struct VersionControlSystem {
    #[serde(rename = "vcs-system")]
    name: Option<String>,
    #[serde(rename = "vcs-version")]
    version: Option<String>,
    #[serde(rename = "vcs-url")]
    url: Option<String>, // todo: use URL type?
    #[serde(rename = "vcs-date")]
    date: Option<String>, // todo: use date type?
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Deserialize, Serialize)]
pub struct License {
    #[serde(rename = "license-name")]
    name: Option<String>,
    #[serde(rename = "license")]
    path: Option<String>,
}

mod test {
    use crate::api::Api;

    #[test]
    fn namespaces() {
        let api = Api::new("https://de.wikipedia.org/w/api.php").unwrap();
        let site_info = api.get_site_info();
        assert!(site_info.is_some());
        let site_info = site_info.unwrap();
        assert_eq!(site_info.namespace_info_by_id(0).unwrap().name, "");
        assert_eq!(
            site_info.namespace_info_by_id(1).as_ref().unwrap().canonical.as_ref().unwrap(),
            "Talk"
        );
        assert_eq!(
            site_info.namespace_info_by_id(1).unwrap().name,
            "Diskussion"
        );
    }
}
