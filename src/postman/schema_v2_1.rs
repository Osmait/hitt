use serde::{Deserialize, Serialize};

pub const SCHEMA_V2_1: &str =
    "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanCollection {
    pub info: PostmanInfo,
    pub item: Vec<PostmanItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable: Option<Vec<PostmanVariable>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<PostmanAuth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanInfo {
    pub name: String,
    #[serde(rename = "_postman_id", skip_serializing_if = "Option::is_none")]
    pub postman_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub schema: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum PostmanItem {
    Folder {
        name: String,
        item: Vec<PostmanItem>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        auth: Option<PostmanAuth>,
    },
    Request {
        name: String,
        request: PostmanRequest,
        #[serde(skip_serializing_if = "Option::is_none")]
        response: Option<Vec<serde_json::Value>>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    pub url: PostmanUrl,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<Vec<PostmanHeader>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<PostmanBody>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<PostmanAuth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PostmanUrl {
    String(String),
    Object {
        raw: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        protocol: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        host: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        query: Option<Vec<PostmanQueryParam>>,
    },
}

impl PostmanUrl {
    pub fn raw(&self) -> &str {
        match self {
            PostmanUrl::String(s) => s,
            PostmanUrl::Object { raw, .. } => raw,
        }
    }

    pub fn query_params(&self) -> Vec<PostmanQueryParam> {
        match self {
            PostmanUrl::Object { query: Some(q), .. } => q.clone(),
            _ => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanQueryParam {
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanHeader {
    pub key: String,
    pub value: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub header_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanBody {
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub urlencoded: Option<Vec<PostmanFormParam>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formdata: Option<Vec<PostmanFormParam>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graphql: Option<PostmanGraphQL>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<PostmanBodyOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanFormParam {
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub param_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanGraphQL {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanBodyOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<PostmanRawOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanRawOptions {
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanAuth {
    #[serde(rename = "type")]
    pub auth_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearer: Option<Vec<PostmanAuthParam>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub basic: Option<Vec<PostmanAuthParam>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apikey: Option<Vec<PostmanAuthParam>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth2: Option<Vec<PostmanAuthParam>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanAuthParam {
    pub key: String,
    pub value: serde_json::Value,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub param_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanVariable {
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub var_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// Environment schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanEnvironment {
    pub id: String,
    pub name: String,
    pub values: Vec<PostmanEnvValue>,
    #[serde(
        rename = "_postman_variable_scope",
        skip_serializing_if = "Option::is_none"
    )]
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanEnvValue {
    pub key: String,
    pub value: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub value_type: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}
