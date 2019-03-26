/*!
The `Api` class serves as a univeral interface to a MediaWiki API.
*/

extern crate cookie;
extern crate reqwest;

use cookie::{Cookie, CookieJar};
use serde_json::Value;
use std::collections::HashMap;

#[macro_export]
/// To quickle create a hashmap.
/// Example: `hashmap!["action"=>"query","meta"=>"siteinfo","siprop"=>"general|namespaces|namespacealiases|libraries|extensions|statistics"]`
macro_rules! hashmap {
    ($( $key: expr => $val: expr ),*) => {{
         let mut map = ::std::collections::HashMap::new();
         $( map.insert($key, $val); )*
         map
    }}
}

/// `MWuser` contains the login data for the `Api`
#[derive(Debug)]
struct MWuser {
    lgusername: String,
    lguserid: u64,
    is_logged_in: bool,
}

impl MWuser {
    /// Returns a new, blank, not-logged-in user
    pub fn new() -> MWuser {
        MWuser {
            lgusername: "".into(),
            lguserid: 0,
            is_logged_in: false,
        }
    }

    /// Tries to set user information from the `Api` call
    pub fn set_from_login(&mut self, login: &serde_json::Value) -> Result<(), String> {
        if login["result"] == "Success" {
            match login["lgusername"].as_str() {
                Some(s) => self.lgusername = s.to_string(),
                None => return Err("No lgusername in login result".to_string()),
            }
            match login["lguserid"].as_u64() {
                Some(u) => self.lguserid = u,
                None => return Err("No lguserid in login result".to_string()),
            }

            self.is_logged_in = true;
        } else {
            self.is_logged_in = false;
        }
        Ok(())
    }
}

/// `Api` is the main class to interact with a MediaWiki API
#[derive(Debug)]
pub struct Api {
    api_url: String,
    site_info: Value,
    client: reqwest::Client,
    cookie_jar: CookieJar,
    user: MWuser,
}

impl Api {
    /// Returns a new `Api` element, and loads the MediaWiki site info from the `api_url` site.
    /// This is done both to get basic information about the site, and to test the API.
    pub fn new(api_url: &str) -> Result<Api, Box<::std::error::Error>> {
        let mut ret = Api {
            api_url: api_url.to_string(),
            site_info: serde_json::from_str(r"{}")?,
            client: reqwest::Client::builder().build()?,
            cookie_jar: CookieJar::new(),
            user: MWuser::new(),
        };
        ret.load_site_info()?;
        Ok(ret)
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
            (a @ &mut Value::Object(_), Value::Object(b)) => {
                let a = a.as_object_mut().unwrap();
                for (k, v) in b {
                    self.json_merge(a.entry(k).or_insert(Value::Null), v);
                }
            }
            (a @ &mut Value::Array(_), Value::Array(b)) => {
                let a = a.as_array_mut().unwrap();
                for v in b {
                    a.push(v);
                }
            }
            (a, b) => *a = b,
        }
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
            serde_json::Value::String(s) => Ok(s.to_string()),
            _ => Err(From::from("Could not get token")),
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
                            let x = v.as_str().unwrap().to_string();
                            cont.insert(k.clone(), x);
                        }
                    }
                }
                _ => {
                    break;
                }
            }
        }
        ret.as_object_mut().unwrap().remove("continue");
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
        params.insert("format".to_string(), "json".to_string());
        let t = self.query_api_raw(&params, method)?;
        let v: Value = serde_json::from_str(&t)?;
        Ok(v)
    }

    /// Runs a query against the MediaWiki API, using `method` GET or POST.
    /// Parameters are a hashmap; `format=json` is enforced.
    fn query_api_json_mut(
        &mut self,
        params: &HashMap<String, String>,
        method: &str,
    ) -> Result<Value, Box<::std::error::Error>> {
        let mut params = params.clone();
        params.insert("format".to_string(), "json".to_string());
        let t = self.query_api_raw_mut(&params, method)?;
        let v: Value = serde_json::from_str(&t)?;
        Ok(v)
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
            .map(|v| v.to_str().unwrap().to_string())
            .collect::<Vec<String>>();
        for cs in cookie_strings {
            let cookie = Cookie::parse(cs.clone()).unwrap();
            self.cookie_jar.add(cookie);
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

    pub fn get_api_request_builder(
        &self,
        params: &HashMap<String, String>,
        method: &str,
    ) -> Result<reqwest::RequestBuilder, Box<::std::error::Error>> {
        self.get_request_builder(&self.api_url.clone(), params, method)
    }

    fn get_request_builder(
        &self,
        api_url: &str,
        params: &HashMap<String, String>,
        method: &str,
    ) -> Result<reqwest::RequestBuilder, Box<::std::error::Error>> {
        let mut req;
        if method == "GET" {
            req = self
                .client
                .get(api_url)
                .header(reqwest::header::COOKIE, self.cookies_to_string())
                .query(&params);
        } else if method == "POST" {
            req = self
                .client
                .post(api_url)
                .header(reqwest::header::COOKIE, self.cookies_to_string())
                .form(&params);
        } else {
            panic!("Unsupported method");
        }
        Ok(req)
    }

    fn query_raw_response(
        &self,
        api_url: &str,
        params: &HashMap<String, String>,
        method: &str,
    ) -> Result<reqwest::Response, Box<::std::error::Error>> {
        let req = self.get_request_builder(api_url, params, method)?;
        let resp = req.send()?;
        return Ok(resp);
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
    /// If successful, user information is stored in `MWuser`, and in the cookie jar
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
            Ok(())
        } else {
            Err(From::from("Login failed"))
        }
    }

    /// Performs a SPARQL query against a wikibase installation.
    /// Tries to get the SPARQL endpoint URL from the site info
    pub fn sparql_query(&self, query: &str) -> Result<Value, Box<::std::error::Error>> {
        let query_api_url = self.get_site_info_string("general", "wikibase-sparql")?;
        let params = hashmap!["query".to_string()=>query.to_string(),"format".to_string()=>"json".to_string()];
        let result = self.query_raw(&query_api_url, &params, "GET")?;
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

    pub fn entities_from_sparql_result(
        &self,
        sparql_result: &serde_json::Value,
        variable_name: &str,
    ) -> Vec<String> {
        let mut entities = vec![];
        for b in sparql_result["results"]["bindings"].as_array().unwrap() {
            match b[variable_name]["value"].as_str() {
                Some(entity_url) => {
                    entities.push(self.extract_entity_from_uri(entity_url).unwrap());
                }
                None => {}
            }
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
