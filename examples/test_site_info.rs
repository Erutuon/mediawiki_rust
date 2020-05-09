#![allow(unused, unused_must_use)]
use mediawiki::{api::Api, params_map, siteinfo::SiteInfo, traits::Errorable};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::error::Error;

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum SiteInfoOrError {
    SiteInfo(SiteInfo),
    Error(Value),
}

impl Errorable for SiteInfoOrError {
    fn get_error(&self) -> Option<&Map<String, Value>> {
        match self {
            SiteInfoOrError::SiteInfo(site_info) => site_info.get_error(),
            _ => None,
        }
    }
}

fn get_site_info(api: Api) -> Result<SiteInfo, Box<dyn Error>> {
    let params = params_map! {
        "action" => "query",
        "meta" => "siteinfo",
        "siprop" => "general|namespaces|namespacealiases|libraries|extensions|statistics",
        "formatversion" => "2",
    };
    Ok(api.get_query_api_json(&params)?)
}

fn main() {
    let host = std::env::args().nth(1).expect("supply hostname for API");
    let api = Api::new(&format!("https://{}/w/api.php", host)).unwrap();
    let site_info = get_site_info(api);
    dbg!(site_info);
}