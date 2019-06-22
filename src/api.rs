/*!
The `Api` class serves as a univeral interface to a MediaWiki API.
*/

#![deny(
    missing_docs,
    missing_debug_implementations,
//    missing_copy_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]

extern crate base64;
extern crate cookie;
extern crate crypto;
extern crate reqwest;

use crate::title::Title;
use crate::user::User;
use cookie::{Cookie, CookieJar};
use crypto::mac::Mac;
use crypto::sha1::Sha1;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::Value;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{thread, time};
use url::Url;
use urlencoding;
use uuid::Uuid;

/// Alias for a namespace (could be -1 for Special pages etc.)
pub type NamespaceID = i64;

const DEFAULT_USER_AGENT: &str = "Rust mediawiki API";
const DEFAULT_MAXLAG: Option<u64> = Some(5);
const MAX_RETRY_ATTEMPTS: u64 = 5;

#[macro_export]
/// To quickly create a hashmap.
/// Example: `hashmap!["action"=>"query","meta"=>"siteinfo","siprop"=>"general|namespaces|namespacealiases|libraries|extensions|statistics"]`
macro_rules! hashmap {
    ($( $key: expr => $val: expr ),*) => {{
         let mut map = ::std::collections::HashMap::new();
         $( map.insert($key, $val); )*
         map
    }}
}

/// Upstream is a trait to wrap multiple supstream sources.
/// By default, this will be UpstreamReqwestClient, but you can create new ones, e.g. for debugging
pub trait Upstream: std::fmt::Debug + Sync {
    // TODO:
    // + Clone
    // fn clone(&self) -> Self where Self: Sized; // ???

    /// GET method
    fn get(
        &self,
        api_url: &str,
        headers: HeaderMap,
        params: &HashMap<String, String>,
    ) -> reqwest::RequestBuilder;

    /// POST method
    fn post(
        &self,
        api_url: &str,
        headers: HeaderMap,
        params: &HashMap<String, String>,
    ) -> reqwest::RequestBuilder;
}

/// An implementation of the `Upstream` trait, using a reqwest client
#[derive(Debug, Clone)]
pub struct UpstreamReqwestClient {
    client: reqwest::Client,
}

impl Upstream for UpstreamReqwestClient {
    fn get(
        &self,
        api_url: &str,
        headers: HeaderMap,
        params: &HashMap<String, String>,
    ) -> reqwest::RequestBuilder {
        self.client.get(api_url).headers(headers).query(&params)
    }

    fn post(
        &self,
        api_url: &str,
        headers: HeaderMap,
        params: &HashMap<String, String>,
    ) -> reqwest::RequestBuilder {
        self.client.post(api_url).headers(headers).form(&params)
    }
}

impl UpstreamReqwestClient {
    /// Returns a new `UpstreamReqwestClient` object, using a reqwest client
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

/// `OAuthParams` contains parameters for OAuth requests
#[derive(Debug, Clone)]
pub struct OAuthParams {
    g_consumer_key: Option<String>,
    g_consumer_secret: Option<String>,
    g_token_key: Option<String>,
    g_token_secret: Option<String>,
    g_user_agent: Option<String>,
    agent: Option<String>,
    consumer_key: Option<String>,
    consumer_secret: Option<String>,
    api_url: Option<String>,
    public_mw_oauth_url: Option<String>,
    tool: Option<String>,
}

impl OAuthParams {
    /// Imports data from JSON stored in the QuickStatements DB batch_oauth.serialized_json field
    pub fn new_from_json(j: &Value) -> Self {
        Self {
            g_consumer_key: j["gConsumerKey"].as_str().map(|s| s.to_string()),
            g_consumer_secret: j["gConsumerSecret"].as_str().map(|s| s.to_string()),
            g_token_key: j["gTokenKey"].as_str().map(|s| s.to_string()),
            g_token_secret: j["gTokenSecret"].as_str().map(|s| s.to_string()),
            g_user_agent: j["gUserAgent"].as_str().map(|s| s.to_string()),
            agent: j["params"]["agent"].as_str().map(|s| s.to_string()),
            consumer_key: j["params"]["consumerKey"].as_str().map(|s| s.to_string()),
            consumer_secret: j["params"]["consumerSecret"]
                .as_str()
                .map(|s| s.to_string()),
            api_url: j["apiUrl"].as_str().map(|s| s.to_string()),
            public_mw_oauth_url: j["publicMwOAuthUrl"].as_str().map(|s| s.to_string()),
            tool: j["tool"].as_str().map(|s| s.to_string()),
        }
    }
}

/// `Api` is the main class to interact with a MediaWiki API
#[derive(Debug)] // Clone
pub struct Api {
    api_url: String,
    site_info: Value,
    /// something something
    upstream: Box<dyn Upstream>,
    cookie_jar: CookieJar,
    user: User,
    user_agent: String,
    maxlag_seconds: Option<u64>,
    edit_delay_ms: Option<u64>,
    oauth: Option<OAuthParams>,
}

impl Api {
    /// Returns a new `Api` element, and loads the MediaWiki site info from the `api_url` site.
    /// This is done both to get basic information about the site, and to test the API.
    pub fn new(api_url: &str) -> Result<Api, Box<::std::error::Error>> {
        Api::new_from_builder(api_url, reqwest::Client::builder())
    }

    /// Returns a new `Api` element, and loads the MediaWiki site info from the `api_url` site.
    /// This is done both to get basic information about the site, and to test the API.
    /// Uses a bespoke reqwest::ClientBuilder.
    pub fn new_from_builder(
        api_url: &str,
        builder: reqwest::ClientBuilder,
    ) -> Result<Api, Box<::std::error::Error>> {
        Api::new_from_upstream(
            api_url,
            Box::new(UpstreamReqwestClient::new(builder.build()?)),
        )
    }

    /// Returns a new `Api` element, and loads the MediaWiki site info from the `api_url` site.
    /// This is done both to get basic information about the site, and to test the API.
    /// Uses an instance with the `Upstream` trait.
    pub fn new_from_upstream(
        api_url: &str,
        upstream: Box<dyn Upstream>,
    ) -> Result<Api, Box<::std::error::Error>> {
        let mut ret = Api {
            api_url: api_url.to_string(),
            site_info: serde_json::from_str(r"{}")?,
            upstream: upstream,
            cookie_jar: CookieJar::new(),
            user: User::new(),
            user_agent: DEFAULT_USER_AGENT.to_string(),
            maxlag_seconds: DEFAULT_MAXLAG,
            edit_delay_ms: None,
            oauth: None,
        };
        ret.load_site_info()?;
        Ok(ret)
    }

    /// Returns a reference to the upstream trait instance
    pub fn upstream(&self) -> &Box<dyn Upstream> {
        &self.upstream
    }

    /// Sets the upstream instance
    pub fn set_upstream(self: &mut Self, upstream: Box<dyn Upstream>) {
        self.upstream = upstream;
    }

    /// Sets the OAuth parameters
    pub fn set_oauth(&mut self, oauth: Option<OAuthParams>) {
        self.oauth = oauth;
    }

    /// Returns a reference to the current OAuth parameters
    pub fn oauth(&self) -> &Option<OAuthParams> {
        &self.oauth
    }

    /// Returns a reference to the current user object
    pub fn user(&self) -> &User {
        &self.user
    }

    /// Returns a mutable reference to the current user object
    pub fn user_mut(&mut self) -> &mut User {
        &mut self.user
    }

    /// Loads the current user info; returns Ok(()) is successful
    pub fn load_user_info(&mut self) -> Result<(), Box<::std::error::Error>> {
        let mut user = self.user.clone();
        user.load_user_info(&self)?;
        self.user = user;
        Ok(())
    }

    /// Returns a reference to the serde_json Value containing the site info
    pub fn get_site_info(&self) -> &Value {
        return &self.site_info;
    }

    /// Returns a serde_json Value in site info, within the `["query"]` object.
    /// The value is a cloned copy.
    pub fn get_site_info_value(&self, k1: &str, k2: &str) -> Value {
        let site_info = self.get_site_info();
        site_info["query"][k1][k2].clone()
    }

    /// Returns a String from the site info, matching `["query"][k1][k2]`
    pub fn get_site_info_string(&self, k1: &str, k2: &str) -> Result<String, String> {
        let site_info = self.get_site_info();
        match site_info["query"][k1][k2].as_str() {
            Some(s) => Ok(s.to_string()),
            None => Err(format!("No 'query.{}.{}' value in site info", k1, k2)),
        }
    }

    /// Returns the canonical namespace name for a namespace ID, if defined
    pub fn get_canonical_namespace_name(&self, namespace_id: NamespaceID) -> Option<String> {
        let v = self.get_site_info_value("namespaces", format!("{}", namespace_id).as_str());
        match v["canonical"].as_str() {
            Some(v) => Some(v.to_string()),
            None => {
                match v["*"].as_str() {
                    Some(c) => Some(c.to_string()), // Main name space, no canonical name
                    None => None,
                }
            }
        }
    }

    /// Loads the site info.
    /// Should only ever be called from `new()`
    fn load_site_info(&mut self) -> Result<&Value, Box<::std::error::Error>> {
        let params = hashmap!["action".to_string()=>"query".to_string(),"meta".to_string()=>"siteinfo".to_string(),"siprop".to_string()=>"general|namespaces|namespacealiases|libraries|extensions|statistics".to_string()];
        self.site_info = self.get_query_api_json(&params)?;
        Ok(&self.site_info)
    }

    /// Merges two JSON objects that are MediaWiki API results.
    /// If an array already exists in the `a` object, it will be expanded with the array from the `b` object
    /// This allows for combining multiple API results via the `continue` parameter
    fn json_merge(&self, a: &mut Value, b: Value) {
        match (a, b) {
            (a @ &mut Value::Object(_), Value::Object(b)) => match a.as_object_mut() {
                Some(a) => {
                    for (k, v) in b {
                        self.json_merge(a.entry(k).or_insert(Value::Null), v);
                    }
                }
                None => {}
            },
            (a @ &mut Value::Array(_), Value::Array(b)) => match a.as_array_mut() {
                Some(a) => {
                    for v in b {
                        a.push(v);
                    }
                }
                None => {}
            },
            (a, b) => *a = b,
        }
    }

    /// Turns a Vec of str tuples into a Hashmap of String, to be used in API calls
    pub fn params_into(&self, params: &Vec<(&str, &str)>) -> HashMap<String, String> {
        params
            .into_iter()
            .map(|tuple| (tuple.0.to_string(), tuple.1.to_string()))
            .collect()
    }

    /// Returns an empty parameter HashMap
    pub fn no_params(&self) -> HashMap<String, String> {
        self.params_into(&vec![])
    }

    /// Returns a token of a `token_type`, such as `login` or `csrf` (for editing)
    pub fn get_token(&mut self, token_type: &str) -> Result<String, Box<::std::error::Error>> {
        let mut params = hashmap!["action".to_string()=>"query".to_string(),"meta".to_string()=>"tokens".to_string()];
        if token_type.len() != 0 {
            params.insert("type".to_string(), token_type.to_string());
        }
        let mut key = token_type.to_string();
        key += &"token".to_string();
        if token_type.len() == 0 {
            key = "csrftoken".into()
        }
        let x = self.query_api_json_mut(&params, "GET")?;
        match &x["query"]["tokens"][&key] {
            Value::String(s) => Ok(s.to_string()),
            _ => Err(From::from(format!("Could not get token: {:?}", x))),
        }
    }

    /// Calls `get_token()` to return an edit token
    pub fn get_edit_token(&mut self) -> Result<String, Box<::std::error::Error>> {
        self.get_token("csrf")
    }

    /// Same as `get_query_api_json` but automatically loads more results via the `continue` parameter
    pub fn get_query_api_json_all(
        &self,
        params: &HashMap<String, String>,
    ) -> Result<Value, Box<::std::error::Error>> {
        let mut cont = HashMap::<String, String>::new();
        let mut ret = serde_json::json!({});
        loop {
            let mut params_cont = params.clone();
            for (k, v) in &cont {
                params_cont.insert(k.to_string(), v.to_string());
            }
            let result = self.get_query_api_json(&params_cont)?;
            cont.clear();
            let conti = result["continue"].clone();
            self.json_merge(&mut ret, result);
            match conti {
                Value::Object(obj) => {
                    for (k, v) in obj {
                        if k != "continue" {
                            match v.as_str() {
                                Some(x) => {
                                    cont.insert(k.clone(), x.to_string());
                                }
                                None => {}
                            }
                        }
                    }
                }
                _ => {
                    break;
                }
            }
        }
        match ret.as_object_mut() {
            Some(x) => {
                x.remove("continue");
            }
            None => {}
        }

        Ok(ret)
    }

    /// Runs a query against the MediaWiki API, using `method` GET or POST.
    /// Parameters are a hashmap; `format=json` is enforced.
    pub fn query_api_json(
        &self,
        params: &HashMap<String, String>,
        method: &str,
    ) -> Result<Value, Box<::std::error::Error>> {
        let mut params = params.clone();
        let mut attempts_left = MAX_RETRY_ATTEMPTS;
        params.insert("format".to_string(), "json".to_string());
        self.set_maxlag_params(&mut params, method);
        loop {
            let t = self.query_api_raw(&params, method)?;
            let v: Value = serde_json::from_str(&t)?;
            match self.check_maxlag(&v) {
                Some(lag_seconds) => {
                    if attempts_left == 0 {
                        return Err(From::from("Max attempts reached [MAXLAG]"));
                    }
                    attempts_left -= 1;
                    thread::sleep(time::Duration::from_millis(1000 * lag_seconds));
                }
                None => return Ok(v),
            }
        }
    }

    /// Runs a query against the MediaWiki API, using `method` GET or POST.
    /// Parameters are a hashmap; `format=json` is enforced.
    fn query_api_json_mut(
        &mut self,
        params: &HashMap<String, String>,
        method: &str,
    ) -> Result<Value, Box<::std::error::Error>> {
        let mut params = params.clone();
        let mut attempts_left = MAX_RETRY_ATTEMPTS;
        params.insert("format".to_string(), "json".to_string());
        self.set_maxlag_params(&mut params, method);
        loop {
            let t = self.query_api_raw_mut(&params, method)?;
            let v: Value = serde_json::from_str(&t)?;
            match self.check_maxlag(&v) {
                Some(lag_seconds) => {
                    if attempts_left == 0 {
                        return Err(From::from("Max attempts reached [MAXLAG]"));
                    }
                    attempts_left -= 1;
                    thread::sleep(time::Duration::from_millis(1000 * lag_seconds));
                }
                None => return Ok(v),
            }
        }
    }

    /// Returns the delay time after edits, in milliseconds, if set
    pub fn edit_delay(&self) -> &Option<u64> {
        &self.edit_delay_ms
    }

    /// Sets the delay time after edits in milliseconds (or `None`).
    /// This is independent of, and additional to, MAXLAG
    pub fn set_edit_delay(&mut self, edit_delay_ms: Option<u64>) {
        self.edit_delay_ms = edit_delay_ms;
    }

    /// Returns the maxlag, in seconds, if set
    pub fn maxlag(&self) -> &Option<u64> {
        &self.maxlag_seconds
    }

    /// Sets the maxlag in seconds (or `None`)
    pub fn set_maxlag(&mut self, maxlag_seconds: Option<u64>) {
        self.maxlag_seconds = maxlag_seconds;
    }

    /// Checks if a query is an edit, based on parameters and method (GET/POST)
    fn is_edit_query(&self, params: &HashMap<String, String>, method: &str) -> bool {
        // Editing only through POST (?)
        if method != "POST" {
            return false;
        }
        // Editing requires a token
        if !params.contains_key("token") {
            return false;
        }
        true
    }

    /// Sets the maglag parameter for a query, if necessary
    fn set_maxlag_params(&self, params: &mut HashMap<String, String>, method: &str) {
        if !self.is_edit_query(params, method) {
            return;
        }
        match self.maxlag_seconds {
            Some(maxlag_seconds) => {
                params.insert("maxlag".to_string(), maxlag_seconds.to_string());
            }
            None => {}
        }
    }

    /// Checks for a MAGLAG error, and returns the lag if so
    fn check_maxlag(&self, v: &Value) -> Option<u64> {
        match v["error"]["code"].as_str() {
            Some(code) => match code {
                "maxlag" => v["error"]["lag"].as_u64().or(self.maxlag_seconds), // Current lag, if given, or fallback
                _ => None,
            },
            None => None,
        }
    }

    /// GET wrapper for `query_api_json`
    pub fn get_query_api_json(
        &self,
        params: &HashMap<String, String>,
    ) -> Result<Value, Box<::std::error::Error>> {
        self.query_api_json(params, "GET")
    }

    /// POST wrapper for `query_api_json`
    pub fn post_query_api_json(
        &self,
        params: &HashMap<String, String>,
    ) -> Result<Value, Box<::std::error::Error>> {
        self.query_api_json(params, "POST")
    }

    /// POST wrapper for `query_api_json`.
    /// Requires `&mut self`, for sassion cookie storage
    pub fn post_query_api_json_mut(
        &mut self,
        params: &HashMap<String, String>,
    ) -> Result<Value, Box<::std::error::Error>> {
        self.query_api_json_mut(params, "POST")
    }

    /// Adds or replaces cookies in the cookie jar from a http `Response`
    pub fn set_cookies_from_response(&mut self, resp: &reqwest::Response) {
        let cookie_strings = resp
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|v| match v.to_str() {
                Ok(x) => Some(x.to_string()),
                Err(_) => None,
            })
            .collect::<Vec<String>>();
        for cs in cookie_strings {
            match Cookie::parse(cs.clone()) {
                Ok(cookie) => {
                    self.cookie_jar.add(cookie);
                }
                Err(_) => {}
            }
        }
    }

    /// Generates a single string to pass as COOKIE parameter in a http `Request`
    pub fn cookies_to_string(&self) -> String {
        self.cookie_jar
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<String>>()
            .join("; ")
    }

    /// Runs a query against the MediaWiki API, and returns a text.
    /// Uses `query_raw`
    pub fn query_api_raw(
        &self,
        params: &HashMap<String, String>,
        method: &str,
    ) -> Result<String, Box<::std::error::Error>> {
        self.query_raw(&self.api_url.clone(), params, method)
    }

    /// Runs a query against the MediaWiki API, and returns a text.
    /// Uses `query_raw_mut`
    fn query_api_raw_mut(
        &mut self,
        params: &HashMap<String, String>,
        method: &str,
    ) -> Result<String, Box<::std::error::Error>> {
        self.query_raw_mut(&self.api_url.clone(), params, method)
    }

    /// Generates a `RequestBuilder` for the API URL
    pub fn get_api_request_builder(
        &self,
        params: &HashMap<String, String>,
        method: &str,
    ) -> Result<reqwest::RequestBuilder, Box<::std::error::Error>> {
        self.request_builder(&self.api_url.clone(), params, method)
    }

    /// Returns the user agent name
    pub fn user_agent(&self) -> &String {
        &self.user_agent
    }

    /// Sets the user agent name
    pub fn set_user_agent<S: Into<String>>(&mut self, agent: S) {
        self.user_agent = agent.into();
    }

    /// Returns the user agent string, as it is passed to the API through a HTTP header
    pub fn user_agent_full(&self) -> String {
        let mut ret: String = self.user_agent.to_string();
        ret += &format!(
            "; {}-rust/{}",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION")
        );
        ret
    }

    /// Encodes a string
    fn rawurlencode(&self, s: &String) -> String {
        urlencoding::encode(s)
    }

    /// Signs an OAuth request
    fn sign_oauth_request(
        &self,
        method: &str,
        api_url: &str,
        to_sign: &HashMap<String, String>,
        oauth: &OAuthParams,
    ) -> Result<String, Box<::std::error::Error>> {
        let mut keys: Vec<String> = to_sign.iter().map(|(k, _)| self.rawurlencode(k)).collect();
        keys.sort();

        let ret: Vec<String> = keys
            .iter()
            .filter_map(|k| match to_sign.get(k) {
                Some(k2) => {
                    let v = self.rawurlencode(&k2);
                    Some(k.clone() + &"=".to_string() + &v)
                }
                None => None,
            })
            .collect();

        let url = Url::parse(api_url)?;
        let mut url_string = url.scheme().to_owned() + &"://".to_string();
        url_string += url.host_str().ok_or("url.host_str is None")?;
        match url.port() {
            Some(port) => url_string += &(":".to_string() + &port.to_string()),
            None => {}
        }
        url_string += url.path();

        let ret = self.rawurlencode(&method.to_string())
            + &"&".to_string()
            + &self.rawurlencode(&url_string)
            + &"&".to_string()
            + &self.rawurlencode(&ret.join("&"));

        let key: String = match (&oauth.g_consumer_secret, &oauth.g_token_secret) {
            (Some(g_consumer_secret), Some(g_token_secret)) => {
                self.rawurlencode(g_consumer_secret)
                    + &"&".to_string()
                    + &self.rawurlencode(g_token_secret)
            }
            _ => {
                return Err(From::from("g_consumer_secret or g_token_secret not set"));
            }
        };

        let mut hmac = crypto::hmac::Hmac::new(Sha1::new(), &key.into_bytes());
        hmac.input(&ret.into_bytes());
        let mut bytes = vec![0u8; hmac.output_bytes()];
        hmac.raw_result(bytes.as_mut_slice());
        let ret: String = base64::encode(&bytes);

        Ok(ret)
    }

    /// Returns a signed OAuth POST `RequestBuilder`
    fn oauth_request_builder(
        &self,
        method: &str,
        api_url: &str,
        main_headers: &mut HeaderMap,
        params: &HashMap<String, String>,
    ) -> Result<reqwest::RequestBuilder, Box<::std::error::Error>> {
        let oauth = match &self.oauth {
            Some(oauth) => oauth,
            None => {
                return Err(From::from(
                    "oauth_request_builder called but self.oauth is None",
                ))
            }
        };

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs()
            .to_string();

        let nonce = Uuid::new_v4().to_simple().to_string();

        let mut headers = HeaderMap::new();

        headers.insert(
            "oauth_consumer_key",
            oauth.g_consumer_key.clone().unwrap().parse()?,
        );
        headers.insert("oauth_token", oauth.g_token_key.clone().unwrap().parse()?);
        headers.insert("oauth_version", "1.0".parse()?);
        headers.insert("oauth_nonce", nonce.parse()?);
        headers.insert("oauth_timestamp", timestamp.parse()?);
        headers.insert("oauth_signature_method", "HMAC-SHA1".parse()?);

        // Prepage signing
        let mut to_sign = params.clone();
        for (key, value) in headers.iter() {
            if key == "oauth_signature" {
                continue;
            }
            to_sign.insert(key.to_string(), value.to_str()?.to_string());
        }

        main_headers.insert(
            "oauth_signature",
            self.sign_oauth_request(method, api_url, &to_sign, &oauth)?
                .parse()?,
        );

        // Collapse headers
        let mut header = "OAuth ".to_string();
        let parts: Vec<String> = headers
            .iter()
            .map(|(key, value)| {
                let key = key.to_string();
                let value = value.to_str().unwrap().to_string();
                let key = self.rawurlencode(&key);
                let value = self.rawurlencode(&value);
                key.to_string() + &"=\"".to_string() + &value.to_string() + &"\"".to_string()
            })
            .collect();
        header += &parts.join(", ");

        headers.insert(
            reqwest::header::AUTHORIZATION,
            HeaderValue::from_str(header.as_str())?,
        );

        match method {
            "GET" => Ok(self.upstream.get(api_url, headers, &params)),
            "POST" => Ok(self.upstream.post(api_url, headers, &params)),
            other => panic!("Unsupported method '{}'", other),
        }
    }

    /// Returns a `RequestBuilder` for a generic URL
    fn request_builder(
        &self,
        api_url: &str,
        params: &HashMap<String, String>,
        method: &str,
    ) -> Result<reqwest::RequestBuilder, Box<::std::error::Error>> {
        let mut headers = HeaderMap::new();
        headers.insert(reqwest::header::COOKIE, self.cookies_to_string().parse()?);
        headers.insert(reqwest::header::USER_AGENT, self.user_agent_full().parse()?);

        // Use OAuth if set
        match self.oauth {
            Some(_) => self.oauth_request_builder(method, api_url, &mut headers, params),
            None => match method {
                "GET" => Ok(self.upstream.get(api_url, headers, &params)),
                "POST" => Ok(self.upstream.post(api_url, headers, &params)),
                other => panic!("Unsupported method '{}'", other),
            },
        }
    }

    fn query_raw_response(
        &self,
        api_url: &str,
        params: &HashMap<String, String>,
        method: &str,
    ) -> Result<reqwest::Response, Box<::std::error::Error>> {
        let req = self.request_builder(api_url, params, method)?;
        let resp = req.send()?;
        self.enact_edit_delay(params, method);
        return Ok(resp);
    }

    fn enact_edit_delay(&self, params: &HashMap<String, String>, method: &str) {
        if !self.is_edit_query(params, method) {
            return;
        }
        match self.edit_delay_ms {
            Some(ms) => thread::sleep(time::Duration::from_millis(ms)),
            None => {}
        }
    }

    /// Runs a query against a generic URL, stores cookies, and returns a text
    /// Used for non-stateless queries, such as logins
    fn query_raw_mut(
        &mut self,
        api_url: &String,
        params: &HashMap<String, String>,
        method: &str,
    ) -> Result<String, Box<::std::error::Error>> {
        let mut resp = self.query_raw_response(api_url, params, method)?;
        self.set_cookies_from_response(&resp);
        Ok(resp.text()?)
    }

    /// Runs a query against a generic URL, and returns a text.
    /// Does not store cookies, but also does not require `&self` to be mutable.
    /// Used for simple queries
    pub fn query_raw(
        &self,
        api_url: &str,
        params: &HashMap<String, String>,
        method: &str,
    ) -> Result<String, Box<::std::error::Error>> {
        let mut resp = self.query_raw_response(api_url, params, method)?;
        Ok(resp.text()?)
    }

    /// Performs a login against the MediaWiki API.
    /// If successful, user information is stored in `User`, and in the cookie jar
    pub fn login<S: Into<String>>(
        &mut self,
        lgname: S,
        lgpassword: S,
    ) -> Result<(), Box<::std::error::Error>> {
        let lgname: &str = &lgname.into();
        let lgpassword: &str = &lgpassword.into();
        let lgtoken = self.get_token("login")?;
        let params = hashmap!("action".to_string()=>"login".to_string(),"lgname".to_string()=>lgname.into(),"lgpassword".to_string()=>lgpassword.into(),"lgtoken".to_string()=>lgtoken.into());
        let res = self.query_api_json_mut(&params, "POST")?;
        if res["login"]["result"] == "Success" {
            self.user.set_from_login(&res["login"])?;
            self.load_user_info()
        } else {
            Err(From::from("Login failed"))
        }
    }

    /// From an API result that has a list of entries with "title" and "ns" (e.g. search), returns a vector of `Title` objects.
    pub fn result_array_to_titles(data: &Value) -> Vec<Title> {
        // See if it's the "root" of the result, then try each sub-object separately
        if data.is_object() {
            return data
                .as_object()
                .unwrap() // OK
                .iter()
                .flat_map(|(_k, v)| Api::result_array_to_titles(&v))
                .collect();
        }
        data.as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|v| Title::new_from_api_result(&v))
            .collect()
    }

    /// Performs a SPARQL query against a wikibase installation.
    /// Tries to get the SPARQL endpoint URL from the site info
    pub fn sparql_query(&self, query: &str) -> Result<Value, Box<::std::error::Error>> {
        let query_api_url = self.get_site_info_string("general", "wikibase-sparql")?;
        let params = hashmap!["query".to_string()=>query.to_string(),"format".to_string()=>"json".to_string()];
        let result = self.query_raw(&query_api_url, &params, "POST")?;
        Ok(serde_json::from_str(&result)?)
    }

    /// Given a `uri` (usually, an URL) that points to a Wikibase entity on this MediaWiki installation, returns the item ID
    pub fn extract_entity_from_uri(&self, uri: &str) -> Result<String, Box<::std::error::Error>> {
        let concept_base_uri = self.get_site_info_string("general", "wikibase-conceptbaseuri")?;
        if uri.starts_with(concept_base_uri.as_str()) {
            Ok(uri[concept_base_uri.len()..].to_string())
        } else {
            Err(From::from(format!(
                "{} does not start with {}",
                uri, concept_base_uri
            )))
        }
    }

    /// Returns a vector of entity IDs (as String) from a SPARQL result, given a variable name
    pub fn entities_from_sparql_result(
        &self,
        sparql_result: &Value,
        variable_name: &str,
    ) -> Vec<String> {
        let mut entities = vec![];
        match sparql_result["results"]["bindings"].as_array() {
            Some(bindings) => {
                for b in bindings {
                    match b[variable_name]["value"].as_str() {
                        Some(entity_url) => {
                            entities.push(self.extract_entity_from_uri(entity_url).unwrap());
                        }
                        None => {}
                    }
                }
            }
            None => {}
        }
        entities
    }
}

#[cfg(test)]
mod tests {
    use super::Api;

    #[test]
    fn site_info() {
        let api = Api::new("https://www.wikidata.org/w/api.php").unwrap();
        assert_eq!(
            api.get_site_info_string("general", "sitename").unwrap(),
            "Wikidata"
        );
    }

    #[test]
    fn sparql_query() {
        let api = Api::new("https://www.wikidata.org/w/api.php").unwrap();
        let res = api.sparql_query ( "SELECT ?q ?qLabel ?fellow_id { ?q wdt:P31 wd:Q5 ; wdt:P6594 ?fellow_id . SERVICE wikibase:label { bd:serviceParam wikibase:language '[AUTO_LANGUAGE],en'. } }" ).unwrap() ;
        assert!(res["results"]["bindings"].as_array().unwrap().len() > 300);
    }
}
