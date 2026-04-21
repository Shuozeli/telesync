use crate::error::TelesyncError;
use crate::types::{Account, ApiResponse, Node, Page, PageList, PageViews};

const BASE_URL: &str = "https://api.telegra.ph";

/// HTTP client for the Telegraph API.
///
/// All methods use POST with form-encoded bodies.
/// The `content` field (for create/edit page) is a JSON-serialized string inside a form field.
pub struct TelegraphClient {
    http: reqwest::Client,
    access_token: String,
}

impl TelegraphClient {
    pub fn new(access_token: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            access_token,
        }
    }

    /// Create a new Telegraph account.
    /// Does not require an access token.
    pub async fn create_account(
        &self,
        short_name: &str,
        author_name: Option<&str>,
        author_url: Option<&str>,
    ) -> Result<Account, TelesyncError> {
        let mut params: Vec<(&str, String)> = vec![("short_name", short_name.to_owned())];
        if let Some(name) = author_name {
            params.push(("author_name", name.to_owned()));
        }
        if let Some(url) = author_url {
            params.push(("author_url", url.to_owned()));
        }

        self.post("createAccount", &params).await
    }

    /// Get information about the current account.
    /// `fields` specifies which fields to return (e.g. ["short_name", "page_count"]).
    pub async fn get_account_info(&self, fields: &[&str]) -> Result<Account, TelesyncError> {
        let fields_json = serde_json::to_string(fields)?;
        let params: Vec<(&str, String)> = vec![
            ("access_token", self.access_token.clone()),
            ("fields", fields_json),
        ];

        self.post("getAccountInfo", &params).await
    }

    /// Edit the current account info. All parameters are optional.
    pub async fn edit_account_info(
        &self,
        short_name: Option<&str>,
        author_name: Option<&str>,
        author_url: Option<&str>,
    ) -> Result<Account, TelesyncError> {
        let mut params: Vec<(&str, String)> = vec![("access_token", self.access_token.clone())];
        if let Some(name) = short_name {
            params.push(("short_name", name.to_owned()));
        }
        if let Some(name) = author_name {
            params.push(("author_name", name.to_owned()));
        }
        if let Some(url) = author_url {
            params.push(("author_url", url.to_owned()));
        }

        self.post("editAccountInfo", &params).await
    }

    /// Revoke the current access token. Returns an Account with the new token.
    pub async fn revoke_access_token(&self) -> Result<Account, TelesyncError> {
        let params: Vec<(&str, String)> = vec![("access_token", self.access_token.clone())];

        self.post("revokeAccessToken", &params).await
    }

    /// Create a new Telegraph page.
    pub async fn create_page(
        &self,
        title: &str,
        author_name: Option<&str>,
        author_url: Option<&str>,
        content: &[Node],
        return_content: bool,
    ) -> Result<Page, TelesyncError> {
        let content_json = serde_json::to_string(content)?;
        let mut params: Vec<(&str, String)> = vec![
            ("access_token", self.access_token.clone()),
            ("title", title.to_owned()),
            ("content", content_json),
            ("return_content", return_content.to_string()),
        ];
        if let Some(name) = author_name {
            params.push(("author_name", name.to_owned()));
        }
        if let Some(url) = author_url {
            params.push(("author_url", url.to_owned()));
        }

        self.post("createPage", &params).await
    }

    /// Edit an existing Telegraph page.
    pub async fn edit_page(
        &self,
        path: &str,
        title: &str,
        author_name: Option<&str>,
        author_url: Option<&str>,
        content: &[Node],
        return_content: bool,
    ) -> Result<Page, TelesyncError> {
        let content_json = serde_json::to_string(content)?;
        let mut params: Vec<(&str, String)> = vec![
            ("access_token", self.access_token.clone()),
            ("path", path.to_owned()),
            ("title", title.to_owned()),
            ("content", content_json),
            ("return_content", return_content.to_string()),
        ];
        if let Some(name) = author_name {
            params.push(("author_name", name.to_owned()));
        }
        if let Some(url) = author_url {
            params.push(("author_url", url.to_owned()));
        }

        self.post("editPage", &params).await
    }

    /// Get a Telegraph page by path. Does not require authentication.
    pub async fn get_page(&self, path: &str, return_content: bool) -> Result<Page, TelesyncError> {
        let params: Vec<(&str, String)> = vec![("return_content", return_content.to_string())];

        self.post(&format!("getPage/{path}"), &params).await
    }

    /// Get a list of pages belonging to the current account.
    pub async fn get_page_list(
        &self,
        offset: Option<i64>,
        limit: Option<i64>,
    ) -> Result<PageList, TelesyncError> {
        let mut params: Vec<(&str, String)> = vec![("access_token", self.access_token.clone())];
        if let Some(o) = offset {
            params.push(("offset", o.to_string()));
        }
        if let Some(l) = limit {
            params.push(("limit", l.to_string()));
        }

        self.post("getPageList", &params).await
    }

    /// Get the number of views for a Telegraph page.
    /// Time parameters are optional and filter by specific time periods.
    pub async fn get_views(
        &self,
        path: &str,
        year: Option<i32>,
        month: Option<i32>,
        day: Option<i32>,
        hour: Option<i32>,
    ) -> Result<PageViews, TelesyncError> {
        let mut params: Vec<(&str, String)> = vec![];
        if let Some(y) = year {
            params.push(("year", y.to_string()));
        }
        if let Some(m) = month {
            params.push(("month", m.to_string()));
        }
        if let Some(d) = day {
            params.push(("day", d.to_string()));
        }
        if let Some(h) = hour {
            params.push(("hour", h.to_string()));
        }

        self.post(&format!("getViews/{path}"), &params).await
    }

    /// Send a POST request to the Telegraph API and deserialize the response.
    async fn post<T>(&self, method: &str, params: &[(&str, String)]) -> Result<T, TelesyncError>
    where
        T: serde::de::DeserializeOwned,
    {
        let url = format!("{BASE_URL}/{method}");
        let response = self.http.post(&url).form(params).send().await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(TelesyncError::ApiStatus {
                status: status.as_u16(),
                message: body,
            });
        }

        let api_response: ApiResponse<T> = response.json().await?;
        if api_response.ok {
            api_response.result.ok_or_else(|| {
                TelesyncError::Api("API returned ok=true but no result field".to_owned())
            })
        } else {
            Err(TelesyncError::Api(
                api_response
                    .error
                    .unwrap_or_else(|| "unknown API error".to_owned()),
            ))
        }
    }
}
