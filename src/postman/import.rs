use anyhow::Result;
use uuid::Uuid;

use super::schema_v2_1::*;
use crate::core::auth::{ApiKeyLocation, AuthConfig, OAuth2GrantType};
use crate::core::collection::{Collection, CollectionItem};
use crate::core::request::{HttpMethod, KeyValuePair, Request, RequestBody};

pub fn import_postman_collection(content: &str) -> Result<Collection> {
    let postman: PostmanCollection = serde_json::from_str(content)?;
    let mut collection = Collection::new(&postman.info.name);
    collection.description = postman.info.description.clone();

    // Collection variables
    if let Some(vars) = &postman.variable {
        collection.variables = vars
            .iter()
            .map(|v| {
                let mut kv = KeyValuePair::new(&v.key, &v.value);
                kv.description = v.description.clone();
                kv
            })
            .collect();
    }

    // Collection auth
    collection.auth = postman.auth.as_ref().map(convert_auth);

    // Items
    collection.items = postman
        .item
        .iter()
        .map(convert_item)
        .collect();

    Ok(collection)
}

fn convert_item(item: &PostmanItem) -> CollectionItem {
    match item {
        PostmanItem::Request {
            name,
            request,
            ..
        } => {
            let req = convert_request(name, request);
            CollectionItem::Request(req)
        }
        PostmanItem::Folder {
            name,
            item,
            description,
            auth,
        } => CollectionItem::Folder {
            id: Uuid::new_v4(),
            name: name.clone(),
            items: item.iter().map(convert_item).collect(),
            auth: auth.as_ref().map(convert_auth),
            description: description.clone(),
        },
    }
}

fn convert_request(name: &str, postman_req: &PostmanRequest) -> Request {
    let method = postman_req
        .method
        .as_deref()
        .and_then(HttpMethod::from_str)
        .unwrap_or(HttpMethod::GET);

    let url = postman_req.url.raw().to_string();
    let mut request = Request::new(name, method, url);

    // Headers
    if let Some(headers) = &postman_req.header {
        request.headers = headers
            .iter()
            .map(|h| {
                let mut kv = KeyValuePair::new(&h.key, &h.value);
                kv.enabled = !h.disabled.unwrap_or(false);
                kv.description = h.description.clone();
                kv
            })
            .collect();
    }

    // Query params
    let params = postman_req.url.query_params();
    request.params = params
        .iter()
        .map(|p| {
            let mut kv = KeyValuePair::new(&p.key, &p.value);
            kv.enabled = !p.disabled.unwrap_or(false);
            kv.description = p.description.clone();
            kv
        })
        .collect();

    // Body
    if let Some(body) = &postman_req.body {
        request.body = Some(convert_body(body));
    }

    // Auth
    if let Some(auth) = &postman_req.auth {
        request.auth = Some(convert_auth(auth));
    }

    // Description
    request.description = postman_req.description.clone();

    request
}

fn convert_body(body: &PostmanBody) -> RequestBody {
    match body.mode.as_str() {
        "raw" => {
            let content = body.raw.clone().unwrap_or_default();
            let is_json = body
                .options
                .as_ref()
                .and_then(|o| o.raw.as_ref())
                .map(|r| r.language == "json")
                .unwrap_or(false);

            if is_json || content.trim_start().starts_with('{') || content.trim_start().starts_with('[') {
                RequestBody::Json(content)
            } else {
                RequestBody::Raw {
                    content,
                    content_type: "text/plain".to_string(),
                }
            }
        }
        "urlencoded" => {
            let pairs = body
                .urlencoded
                .as_ref()
                .map(|params| {
                    params
                        .iter()
                        .map(|p| {
                            let mut kv = KeyValuePair::new(&p.key, &p.value);
                            kv.enabled = !p.disabled.unwrap_or(false);
                            kv.description = p.description.clone();
                            kv
                        })
                        .collect()
                })
                .unwrap_or_default();
            RequestBody::FormUrlEncoded(pairs)
        }
        "formdata" => {
            let pairs = body
                .formdata
                .as_ref()
                .map(|params| {
                    params
                        .iter()
                        .map(|p| {
                            let mut kv = KeyValuePair::new(&p.key, &p.value);
                            kv.enabled = !p.disabled.unwrap_or(false);
                            kv.description = p.description.clone();
                            kv
                        })
                        .collect()
                })
                .unwrap_or_default();
            RequestBody::FormData(pairs)
        }
        "graphql" => {
            if let Some(gql) = &body.graphql {
                RequestBody::GraphQL {
                    query: gql.query.clone(),
                    variables: gql.variables.clone(),
                }
            } else {
                RequestBody::None
            }
        }
        _ => RequestBody::None,
    }
}

fn convert_auth(auth: &PostmanAuth) -> AuthConfig {
    match auth.auth_type.as_str() {
        "bearer" => {
            let token = auth
                .bearer
                .as_ref()
                .and_then(|params| {
                    params
                        .iter()
                        .find(|p| p.key == "token")
                        .and_then(|p| p.value.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_default();
            AuthConfig::Bearer { token }
        }
        "basic" => {
            let username = get_auth_param(&auth.basic, "username");
            let password = get_auth_param(&auth.basic, "password");
            AuthConfig::Basic { username, password }
        }
        "apikey" => {
            let key = get_auth_param(&auth.apikey, "key");
            let value = get_auth_param(&auth.apikey, "value");
            let in_value = get_auth_param(&auth.apikey, "in");
            let location = if in_value == "query" {
                ApiKeyLocation::QueryParam
            } else {
                ApiKeyLocation::Header
            };
            AuthConfig::ApiKey {
                key,
                value,
                location,
            }
        }
        "oauth2" => {
            let access_token_url = get_auth_param(&auth.oauth2, "accessTokenUrl");
            let client_id = get_auth_param(&auth.oauth2, "clientId");
            let client_secret = get_auth_param(&auth.oauth2, "clientSecret");
            let scope = {
                let s = get_auth_param(&auth.oauth2, "scope");
                if s.is_empty() { None } else { Some(s) }
            };
            let token = {
                let t = get_auth_param(&auth.oauth2, "accessToken");
                if t.is_empty() { None } else { Some(t) }
            };
            AuthConfig::OAuth2 {
                grant_type: OAuth2GrantType::ClientCredentials,
                access_token_url,
                client_id,
                client_secret,
                scope,
                token,
            }
        }
        "noauth" => AuthConfig::None,
        _ => AuthConfig::None,
    }
}

fn get_auth_param(params: &Option<Vec<PostmanAuthParam>>, key: &str) -> String {
    params
        .as_ref()
        .and_then(|p| {
            p.iter()
                .find(|param| param.key == key)
                .and_then(|param| param.value.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_default()
}
