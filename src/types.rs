use serde::{Deserialize, Serialize};

/// Telegraph Account object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub short_name: String,
    #[serde(default)]
    pub author_name: String,
    #[serde(default)]
    pub author_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_count: Option<i64>,
}

/// Telegraph Page object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub path: String,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<Node>>,
    pub views: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_edit: Option<bool>,
}

/// Telegraph PageList object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageList {
    pub total_count: i64,
    pub pages: Vec<Page>,
}

/// Telegraph PageViews object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageViews {
    pub views: i64,
}

/// A Telegraph DOM Node. Either a text string or a NodeElement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Node {
    Text(String),
    Element(NodeElement),
}

/// A Telegraph DOM element node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeElement {
    pub tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<NodeAttrs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Node>>,
}

/// Attributes for a NodeElement. Telegraph only allows `href` and `src`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeAttrs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
}

/// Telegraph API response wrapper.
#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    pub result: Option<T>,
    pub error: Option<String>,
}
