use std::fmt::Write;

use crate::core::collection::{Collection, CollectionItem};
use crate::core::request::{HttpMethod, Request, RequestBody};

pub fn generate_docs(collection: &Collection) -> String {
    let mut doc = String::new();

    let _ = writeln!(doc, "# {}\n", collection.name);

    if let Some(desc) = &collection.description {
        let _ = writeln!(doc, "{desc}\n");
    }

    // Table of contents
    doc.push_str("## Table of Contents\n\n");
    generate_toc(&collection.items, &mut doc, 0);
    doc.push('\n');

    // Variables
    if !collection.variables.is_empty() {
        doc.push_str("## Variables\n\n");
        doc.push_str("| Variable | Value |\n");
        doc.push_str("|----------|-------|\n");
        for var in &collection.variables {
            let _ = writeln!(doc, "| `{{{{{}}}}}` | `{}` |", var.key, var.value);
        }
        doc.push('\n');
    }

    // Requests
    generate_items_docs(&collection.items, &mut doc, 2);

    doc
}

fn generate_toc(items: &[CollectionItem], doc: &mut String, depth: usize) {
    let indent = "  ".repeat(depth);
    for item in items {
        match item {
            CollectionItem::Request(req) => {
                let _ = writeln!(
                    doc,
                    "{}- {} **{}**",
                    indent,
                    method_badge(req.method),
                    req.name
                );
            }
            CollectionItem::Folder { name, items, .. } => {
                let _ = writeln!(doc, "{indent}- **{name}**");
                generate_toc(items, doc, depth + 1);
            }
        }
    }
}

fn generate_items_docs(items: &[CollectionItem], doc: &mut String, heading_level: usize) {
    for item in items {
        match item {
            CollectionItem::Request(req) => {
                generate_request_doc(req, doc, heading_level);
            }
            CollectionItem::Folder {
                name,
                items,
                description,
                ..
            } => {
                let hashes = "#".repeat(heading_level);
                let _ = writeln!(doc, "{hashes} {name}\n");
                if let Some(desc) = description {
                    let _ = writeln!(doc, "{desc}\n");
                }
                generate_items_docs(items, doc, heading_level + 1);
            }
        }
    }
}

fn generate_request_doc(request: &Request, doc: &mut String, heading_level: usize) {
    let hashes = "#".repeat(heading_level);

    let _ = writeln!(
        doc,
        "{} {} `{}`\n",
        hashes,
        method_badge(request.method),
        request.name
    );

    if let Some(desc) = &request.description {
        let _ = writeln!(doc, "{desc}\n");
    }

    let _ = writeln!(doc, "**URL:** `{} {}`\n", request.method, request.url);

    // Parameters
    if !request.params.is_empty() {
        doc.push_str("**Query Parameters:**\n\n");
        doc.push_str("| Parameter | Value | Description |\n");
        doc.push_str("|-----------|-------|-------------|\n");
        for param in &request.params {
            let desc = param.description.as_deref().unwrap_or("");
            let enabled = if param.enabled { "" } else { " _(disabled)_" };
            let _ = writeln!(
                doc,
                "| `{}` | `{}` | {}{} |",
                param.key, param.value, desc, enabled
            );
        }
        doc.push('\n');
    }

    // Headers
    let user_headers: Vec<_> = request.headers.iter().filter(|h| h.enabled).collect();
    if !user_headers.is_empty() {
        doc.push_str("**Headers:**\n\n");
        doc.push_str("| Header | Value |\n");
        doc.push_str("|--------|-------|\n");
        for header in user_headers {
            let _ = writeln!(doc, "| `{}` | `{}` |", header.key, header.value);
        }
        doc.push('\n');
    }

    // Auth
    if let Some(auth) = &request.auth {
        let _ = writeln!(doc, "**Authentication:** {}\n", auth.display_name());
    }

    // Body
    match &request.body {
        Some(RequestBody::Json(json)) => {
            doc.push_str("**Request Body (JSON):**\n\n");
            doc.push_str("```json\n");
            // Try to pretty-print
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json) {
                doc.push_str(
                    &serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| json.clone()),
                );
            } else {
                doc.push_str(json);
            }
            doc.push_str("\n```\n\n");
        }
        Some(RequestBody::FormUrlEncoded(pairs)) => {
            doc.push_str("**Request Body (Form URL Encoded):**\n\n");
            doc.push_str("| Field | Value |\n");
            doc.push_str("|-------|-------|\n");
            for pair in pairs.iter().filter(|p| p.enabled) {
                let _ = writeln!(doc, "| `{}` | `{}` |", pair.key, pair.value);
            }
            doc.push('\n');
        }
        Some(RequestBody::Raw {
            content,
            content_type,
        }) => {
            let _ = writeln!(doc, "**Request Body ({content_type}):**\n");
            doc.push_str("```\n");
            doc.push_str(content);
            doc.push_str("\n```\n\n");
        }
        Some(RequestBody::GraphQL { query, variables }) => {
            doc.push_str("**GraphQL Query:**\n\n");
            doc.push_str("```graphql\n");
            doc.push_str(query);
            doc.push_str("\n```\n\n");
            if let Some(vars) = variables {
                doc.push_str("**Variables:**\n\n```json\n");
                doc.push_str(vars);
                doc.push_str("\n```\n\n");
            }
        }
        _ => {}
    }

    doc.push_str("---\n\n");
}

fn method_badge(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::GET => "`GET`",
        HttpMethod::POST => "`POST`",
        HttpMethod::PUT => "`PUT`",
        HttpMethod::PATCH => "`PATCH`",
        HttpMethod::DELETE => "`DELETE`",
        HttpMethod::HEAD => "`HEAD`",
        HttpMethod::OPTIONS => "`OPTIONS`",
        HttpMethod::TRACE => "`TRACE`",
    }
}
