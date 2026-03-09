use anyhow::Result;
use uuid::Uuid;

use super::schema_v2_1::*;
use crate::core::auth::AuthConfig;
use crate::core::collection::{Collection, CollectionItem};
use crate::core::request::{Request, RequestBody};

pub fn export_postman_collection(collection: &Collection) -> Result<String> {
    let postman = to_postman_collection(collection);
    let json = serde_json::to_string_pretty(&postman)?;
    Ok(json)
}

fn to_postman_collection(collection: &Collection) -> PostmanCollection {
    PostmanCollection {
        info: PostmanInfo {
            name: collection.name.clone(),
            postman_id: Some(Uuid::new_v4().to_string()),
            description: collection.description.clone(),
            schema: SCHEMA_V2_1.to_string(),
        },
        item: collection.items.iter().map(convert_item).collect(),
        variable: if collection.variables.is_empty() {
            None
        } else {
            Some(
                collection
                    .variables
                    .iter()
                    .map(|v| PostmanVariable {
                        key: v.key.clone(),
                        value: v.value.clone(),
                        var_type: Some("string".to_string()),
                        description: v.description.clone(),
                    })
                    .collect(),
            )
        },
        auth: collection.auth.as_ref().map(convert_auth),
    }
}

fn convert_item(item: &CollectionItem) -> PostmanItem {
    match item {
        CollectionItem::Request(req) => PostmanItem::Request {
            name: req.name.clone(),
            request: convert_request(req),
            response: None,
        },
        CollectionItem::Folder {
            name,
            items,
            auth,
            description,
            ..
        } => PostmanItem::Folder {
            name: name.clone(),
            item: items.iter().map(convert_item).collect(),
            description: description.clone(),
            auth: auth.as_ref().map(convert_auth),
        },
    }
}

fn convert_request(req: &Request) -> PostmanRequest {
    let query: Vec<PostmanQueryParam> = req
        .params
        .iter()
        .map(|p| PostmanQueryParam {
            key: p.key.clone(),
            value: p.value.clone(),
            description: p.description.clone(),
            disabled: if !p.enabled { Some(true) } else { None },
        })
        .collect();

    let url = if query.is_empty() {
        PostmanUrl::String(req.url.clone())
    } else {
        PostmanUrl::Object {
            raw: req.url.clone(),
            protocol: None,
            host: None,
            path: None,
            query: Some(query),
        }
    };

    PostmanRequest {
        method: Some(req.method.as_str().to_string()),
        url,
        header: if req.headers.is_empty() {
            None
        } else {
            Some(
                req.headers
                    .iter()
                    .map(|h| PostmanHeader {
                        key: h.key.clone(),
                        value: h.value.clone(),
                        header_type: Some("text".to_string()),
                        description: h.description.clone(),
                        disabled: if !h.enabled { Some(true) } else { None },
                    })
                    .collect(),
            )
        },
        body: req.body.as_ref().map(convert_body),
        auth: req.auth.as_ref().map(convert_auth),
        description: req.description.clone(),
    }
}

fn convert_body(body: &RequestBody) -> PostmanBody {
    match body {
        RequestBody::Json(json) => PostmanBody {
            mode: "raw".to_string(),
            raw: Some(json.clone()),
            urlencoded: None,
            formdata: None,
            graphql: None,
            options: Some(PostmanBodyOptions {
                raw: Some(PostmanRawOptions {
                    language: "json".to_string(),
                }),
            }),
        },
        RequestBody::FormUrlEncoded(pairs) => PostmanBody {
            mode: "urlencoded".to_string(),
            raw: None,
            urlencoded: Some(
                pairs
                    .iter()
                    .map(|p| PostmanFormParam {
                        key: p.key.clone(),
                        value: p.value.clone(),
                        description: p.description.clone(),
                        disabled: if !p.enabled { Some(true) } else { None },
                        param_type: None,
                    })
                    .collect(),
            ),
            formdata: None,
            graphql: None,
            options: None,
        },
        RequestBody::FormData(pairs) => PostmanBody {
            mode: "formdata".to_string(),
            raw: None,
            urlencoded: None,
            formdata: Some(
                pairs
                    .iter()
                    .map(|p| PostmanFormParam {
                        key: p.key.clone(),
                        value: p.value.clone(),
                        description: p.description.clone(),
                        disabled: if !p.enabled { Some(true) } else { None },
                        param_type: Some("text".to_string()),
                    })
                    .collect(),
            ),
            graphql: None,
            options: None,
        },
        RequestBody::GraphQL { query, variables } => PostmanBody {
            mode: "graphql".to_string(),
            raw: None,
            urlencoded: None,
            formdata: None,
            graphql: Some(PostmanGraphQL {
                query: query.clone(),
                variables: variables.clone(),
            }),
            options: None,
        },
        RequestBody::Raw {
            content,
            content_type,
        } => PostmanBody {
            mode: "raw".to_string(),
            raw: Some(content.clone()),
            urlencoded: None,
            formdata: None,
            graphql: None,
            options: Some(PostmanBodyOptions {
                raw: Some(PostmanRawOptions {
                    language: if content_type.contains("xml") {
                        "xml".to_string()
                    } else if content_type.contains("html") {
                        "html".to_string()
                    } else {
                        "text".to_string()
                    },
                }),
            }),
        },
        _ => PostmanBody {
            mode: "raw".to_string(),
            raw: None,
            urlencoded: None,
            formdata: None,
            graphql: None,
            options: None,
        },
    }
}

fn convert_auth(auth: &AuthConfig) -> PostmanAuth {
    match auth {
        AuthConfig::Bearer { token } => PostmanAuth {
            auth_type: "bearer".to_string(),
            bearer: Some(vec![PostmanAuthParam {
                key: "token".to_string(),
                value: serde_json::Value::String(token.clone()),
                param_type: Some("string".to_string()),
            }]),
            basic: None,
            apikey: None,
            oauth2: None,
        },
        AuthConfig::Basic { username, password } => PostmanAuth {
            auth_type: "basic".to_string(),
            bearer: None,
            basic: Some(vec![
                PostmanAuthParam {
                    key: "username".to_string(),
                    value: serde_json::Value::String(username.clone()),
                    param_type: Some("string".to_string()),
                },
                PostmanAuthParam {
                    key: "password".to_string(),
                    value: serde_json::Value::String(password.clone()),
                    param_type: Some("string".to_string()),
                },
            ]),
            apikey: None,
            oauth2: None,
        },
        AuthConfig::ApiKey {
            key,
            value,
            location,
        } => PostmanAuth {
            auth_type: "apikey".to_string(),
            bearer: None,
            basic: None,
            apikey: Some(vec![
                PostmanAuthParam {
                    key: "key".to_string(),
                    value: serde_json::Value::String(key.clone()),
                    param_type: Some("string".to_string()),
                },
                PostmanAuthParam {
                    key: "value".to_string(),
                    value: serde_json::Value::String(value.clone()),
                    param_type: Some("string".to_string()),
                },
                PostmanAuthParam {
                    key: "in".to_string(),
                    value: serde_json::Value::String(
                        match location {
                            crate::core::auth::ApiKeyLocation::Header => "header",
                            crate::core::auth::ApiKeyLocation::QueryParam => "query",
                        }
                        .to_string(),
                    ),
                    param_type: Some("string".to_string()),
                },
            ]),
            oauth2: None,
        },
        AuthConfig::None | AuthConfig::Inherit => PostmanAuth {
            auth_type: "noauth".to_string(),
            bearer: None,
            basic: None,
            apikey: None,
            oauth2: None,
        },
        AuthConfig::OAuth2 {
            access_token_url,
            client_id,
            client_secret,
            scope,
            token,
            ..
        } => PostmanAuth {
            auth_type: "oauth2".to_string(),
            bearer: None,
            basic: None,
            apikey: None,
            oauth2: Some(vec![
                PostmanAuthParam {
                    key: "accessTokenUrl".to_string(),
                    value: serde_json::Value::String(access_token_url.clone()),
                    param_type: Some("string".to_string()),
                },
                PostmanAuthParam {
                    key: "clientId".to_string(),
                    value: serde_json::Value::String(client_id.clone()),
                    param_type: Some("string".to_string()),
                },
                PostmanAuthParam {
                    key: "clientSecret".to_string(),
                    value: serde_json::Value::String(client_secret.clone()),
                    param_type: Some("string".to_string()),
                },
                PostmanAuthParam {
                    key: "scope".to_string(),
                    value: serde_json::Value::String(scope.clone().unwrap_or_default()),
                    param_type: Some("string".to_string()),
                },
                PostmanAuthParam {
                    key: "accessToken".to_string(),
                    value: serde_json::Value::String(token.clone().unwrap_or_default()),
                    param_type: Some("string".to_string()),
                },
            ]),
        },
    }
}
