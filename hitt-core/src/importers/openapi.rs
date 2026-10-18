use anyhow::Result;
use openapiv3::OpenAPI;

use crate::core::collection::{Collection, CollectionItem};
use crate::core::request::{HttpMethod, KeyValuePair, Request, RequestBody};

pub fn import_openapi(spec: &str) -> Result<Collection> {
    let openapi: OpenAPI = if spec.trim().starts_with('{') {
        serde_json::from_str(spec)?
    } else {
        serde_yaml::from_str(spec)?
    };

    let title = openapi.info.title.clone();
    let mut collection = Collection::new(&title);
    collection.description.clone_from(&openapi.info.description);

    // Extract base URL from servers
    if let Some(server) = openapi.servers.first() {
        collection
            .variables
            .push(KeyValuePair::new("base_url", &server.url));
    }

    // Group paths by tag
    let mut tag_folders: std::collections::HashMap<String, Vec<CollectionItem>> =
        std::collections::HashMap::new();
    let mut untagged = Vec::new();

    for (path, path_item) in &openapi.paths.paths {
        let path_item = match path_item {
            openapiv3::ReferenceOr::Item(item) => item,
            openapiv3::ReferenceOr::Reference { .. } => continue,
        };

        let operations = [
            ("GET", &path_item.get),
            ("POST", &path_item.post),
            ("PUT", &path_item.put),
            ("PATCH", &path_item.patch),
            ("DELETE", &path_item.delete),
            ("HEAD", &path_item.head),
            ("OPTIONS", &path_item.options),
        ];

        for (method_str, op) in operations {
            if let Some(operation) = op {
                #[allow(deprecated)]
                let method = HttpMethod::from_str(method_str).unwrap_or(HttpMethod::GET);
                let name = operation
                    .summary
                    .clone()
                    .or_else(|| operation.operation_id.clone())
                    .unwrap_or_else(|| format!("{method_str} {path}"));

                let url = format!("{{{{base_url}}}}{path}");
                let mut request = Request::new(&name, method, url);
                request.description.clone_from(&operation.description);

                // Extract parameters
                for param_ref in &operation.parameters {
                    if let openapiv3::ReferenceOr::Item(param) = param_ref {
                        match param {
                            openapiv3::Parameter::Query { parameter_data, .. } => {
                                let mut kv = KeyValuePair::new(
                                    &parameter_data.name,
                                    example_value_from_schema(&parameter_data.format),
                                );
                                if let Some(desc) = &parameter_data.description {
                                    kv.description = Some(desc.clone());
                                }
                                kv.enabled = parameter_data.required;
                                request.params.push(kv);
                            }
                            openapiv3::Parameter::Header { parameter_data, .. } => {
                                let mut kv = KeyValuePair::new(
                                    &parameter_data.name,
                                    example_value_from_schema(&parameter_data.format),
                                );
                                if let Some(desc) = &parameter_data.description {
                                    kv.description = Some(desc.clone());
                                }
                                request.headers.push(kv);
                            }
                            openapiv3::Parameter::Path { parameter_data, .. } => {
                                // Path params are embedded in URL template
                                let placeholder = format!("{{{{{}}}}}", parameter_data.name);
                                request.url = request
                                    .url
                                    .replace(&format!("{{{}}}", parameter_data.name), &placeholder);
                            }
                            openapiv3::Parameter::Cookie { .. } => {}
                        }
                    }
                }

                // Extract request body
                if let Some(openapiv3::ReferenceOr::Item(body)) = &operation.request_body {
                    if let Some(json_media) = body.content.get("application/json") {
                        if let Some(schema_ref) = &json_media.schema {
                            let example = schema_to_example_json(schema_ref, &openapi);
                            request.body = Some(RequestBody::Json(
                                serde_json::to_string_pretty(&example).unwrap_or_default(),
                            ));
                            request
                                .headers
                                .push(KeyValuePair::new("Content-Type", "application/json"));
                        }
                    }
                }

                let item = CollectionItem::Request(Box::new(request));

                // Add to appropriate tag folder
                if let Some(tag) = operation.tags.first() {
                    tag_folders.entry(tag.clone()).or_default().push(item);
                } else {
                    untagged.push(item);
                }
            }
        }
    }

    // Build collection structure
    for (tag, items) in tag_folders {
        collection.items.push(CollectionItem::Folder {
            id: uuid::Uuid::new_v4(),
            name: tag,
            items,
            auth: None,
            description: None,
        });
    }
    collection.items.extend(untagged);

    Ok(collection)
}

fn example_value_from_schema(format: &openapiv3::ParameterSchemaOrContent) -> String {
    match format {
        openapiv3::ParameterSchemaOrContent::Schema(schema_ref) => match schema_ref {
            openapiv3::ReferenceOr::Item(schema) => match &schema.schema_kind {
                openapiv3::SchemaKind::Type(t) => match t {
                    openapiv3::Type::String(_) => "example".to_string(),
                    openapiv3::Type::Number(_) | openapiv3::Type::Integer(_) => "0".to_string(),
                    openapiv3::Type::Boolean(_) => "true".to_string(),
                    _ => String::new(),
                },
                _ => String::new(),
            },
            openapiv3::ReferenceOr::Reference { .. } => String::new(),
        },
        openapiv3::ParameterSchemaOrContent::Content(_) => String::new(),
    }
}

#[allow(clippy::only_used_in_recursion)]
fn schema_to_example_json(
    schema_ref: &openapiv3::ReferenceOr<openapiv3::Schema>,
    openapi: &OpenAPI,
) -> serde_json::Value {
    match schema_ref {
        openapiv3::ReferenceOr::Item(schema) => match &schema.schema_kind {
            openapiv3::SchemaKind::Type(t) => match t {
                openapiv3::Type::Object(obj) => {
                    let mut map = serde_json::Map::new();
                    for (name, prop) in &obj.properties {
                        let value = match prop {
                            openapiv3::ReferenceOr::Item(s) => schema_to_example_json(
                                &openapiv3::ReferenceOr::Item(*s.clone()),
                                openapi,
                            ),
                            openapiv3::ReferenceOr::Reference { .. } => {
                                serde_json::Value::String("example".to_string())
                            }
                        };
                        map.insert(name.clone(), value);
                    }
                    serde_json::Value::Object(map)
                }
                openapiv3::Type::Array(arr) => {
                    if let Some(items) = &arr.items {
                        let unboxed: openapiv3::ReferenceOr<openapiv3::Schema> = match items {
                            openapiv3::ReferenceOr::Item(boxed) => {
                                openapiv3::ReferenceOr::Item(*boxed.clone())
                            }
                            openapiv3::ReferenceOr::Reference { reference } => {
                                openapiv3::ReferenceOr::Reference {
                                    reference: reference.clone(),
                                }
                            }
                        };
                        let item = schema_to_example_json(&unboxed, openapi);
                        serde_json::json!([item])
                    } else {
                        serde_json::json!([])
                    }
                }
                openapiv3::Type::String(_) => serde_json::Value::String("example".to_string()),
                openapiv3::Type::Number(_) | openapiv3::Type::Integer(_) => serde_json::json!(0),
                openapiv3::Type::Boolean(_) => serde_json::json!(true),
            },
            _ => serde_json::Value::Null,
        },
        openapiv3::ReferenceOr::Reference { .. } => serde_json::Value::String("$ref".to_string()),
    }
}
