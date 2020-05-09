/*!
The `Page` class deals with operations done on pages, like editing.
*/

#![deny(
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]

extern crate lazy_static;

use crate::api::Api;
use crate::params_map;
use crate::title::Title;
use serde_json::Value;
use std::error::Error;
use std::fmt;

/// Represents a page.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Page {
    title: Title,
}

impl Page {
    /// Creates a new `Page` from a `Title`.
    pub fn new(title: Title) -> Self {
        Page { title }
    }

    /// Accesses the `Title` of this `Page`.
    pub fn title(&self) -> &Title {
        &self.title
    }

    /// Fetches the current text of this `Page`. If there is one slot in
    /// the current revision, it is fetched; if there are multiple slots,
    /// the "main" slot is fetched, or an error is returned if there is
    /// no "main" slot.
    ///
    /// # Errors
    /// May return a `PageError` or any error from [`Api::get_query_api_json`].
    ///
    /// [`Api::get_query_api_json`]: ../api/struct.Api.html#method.get_query_api_json
    pub fn text(&self, api: &Api) -> Result<String, PageError> {
        let title = self
            .title
            .full_pretty(api)
            .ok_or_else(|| PageError::BadTitle(self.title.clone()))?;
        let params = params_map! {
            "action" => "query",
            "prop" => "revisions",
            "titles" => &title,
            "rvslots" => "*",
            "rvprop" => "content",
            "formatversion" => "2",
        };
        let mut result: Value = api
            .get_query_api_json(&params)
            .map_err(PageError::RequestError)?;

        let mut page = result["query"]["pages"][0].take();
        if page["missing"].as_bool() == Some(true) {
            Err(PageError::Missing(self.title.clone()))
        } else if let Value::Object(mut slots) =
            page["revisions"][0]["slots"].take()
        {
            slots
                .get_mut("main")
                .map(|main_slot| main_slot.take())
                .or_else(|| {
                    slots
                        .values_mut()
                        .next()
                        .map(|first_slot| first_slot.take())
                })
                .map(|mut slot| {
                    if let Value::String(s) = slot["content"].take() {
                        Some(s)
                    } else {
                        None
                    }
                })
                .flatten()
                .ok_or_else(|| PageError::BadResponse(result))
        } else {
            Err(PageError::BadResponse(result))
        }
    }

    /// Edits this `Page` with the given parameters and edit summary.
    ///
    /// # Errors
    /// May return a `PageError` or any error from [`Api::post_query_api_json`].
    ///
    /// [`Api::post_query_api_json`]: ../api/struct.Api.html#method.post_query_api_json
    pub fn edit_text(
        &self,
        api: &mut Api,
        text: impl Into<String>,
        summary: impl Into<String>,
    ) -> Result<(), Box<dyn Error>> {
        let title = self
            .title
            .full_pretty(api)
            .ok_or_else(|| PageError::BadTitle(self.title.clone()))?;
        let bot = if api.user().is_bot() { "true" } else { "false" };
        let mut params = params_map! {
            "action" => "edit",
            "title" => title,
            "text" => text,
            "summary" => summary,
            "bot" => bot,
            "formatversion" => "2",
            "token" => api.get_edit_token()?,
        };

        if !api.user().user_name().is_empty() {
            params.insert("assert".to_string(), "user".to_string());
        }

        let result = api.post_query_api_json(&params)?;
        if result["edit"].as_str() == Some("Success") {
            Ok(())
        } else {
            Err(PageError::EditError(result).into())
        }
    }
}

/// Errors that can go wrong while performing operations on a `Page`.
#[derive(Debug)]
#[non_exhaustive]
pub enum PageError {
    /// Couldn't obtain the title for this page for use in an API request.
    BadTitle(Title),

    /// Couldn't understand the API response (provided).
    BadResponse(Value),

    /// Missing page.
    Missing(Title),

    /// Edit failed; API response is provided.
    EditError(Value),

    /// Error while performing the API request.
    RequestError(Box<dyn Error>),
}

impl fmt::Display for PageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PageError::BadTitle(title) => {
                write!(f, "invalid title for this Page: {:?}", title)
            }
            PageError::BadResponse(response) => write!(
                f,
                "bad API response while fetching revision content: {:?}",
                response
            ),
            PageError::Missing(title) => write!(f, "page missing: {:?}", title),
            PageError::EditError(response) => {
                write!(f, "edit resulted in error: {:?}", response)
            }
            PageError::RequestError(error) => {
                write!(f, "request error: {}", error)
            }
        }
    }
}

impl Error for PageError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::*;

    lazy_static! {
        static ref WD_API: Api =
            Api::new("https://www.wikidata.org/w/api.php").unwrap();
    }

    #[test]
    fn page_text_main_page_nonempty() {
        let page = Page::new(Title::new("Main Page", 4));
        let text = page.text(&WD_API);
        assert!(text.is_ok() && !text.unwrap().is_empty());
    }

    #[test]
    fn page_text_nonexistent() {
        let title = Title::new("This page does not exist", 0);
        let page = Page::new(title.clone());
        assert!(
            matches!(page.text(&WD_API), Err(PageError::Missing(t)) if t == title)
        );
    }
}
