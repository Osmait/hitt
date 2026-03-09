use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct GrpcSession {
    pub id: Uuid,
    pub url: String,
    pub proto_file: Option<String>,
    pub services: Vec<GrpcService>,
    pub selected_service: Option<usize>,
    pub selected_method: Option<usize>,
    pub request_body: String,
    pub response_body: Option<String>,
    pub status: GrpcStatus,
}

impl GrpcSession {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            url: url.into(),
            proto_file: None,
            services: Vec::new(),
            selected_service: None,
            selected_method: None,
            request_body: "{}".to_string(),
            response_body: None,
            status: GrpcStatus::Idle,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum GrpcStatus {
    Idle,
    Loading,
    Ready,
    Sending,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct GrpcService {
    pub name: String,
    pub methods: Vec<GrpcMethod>,
}

#[derive(Debug, Clone)]
pub struct GrpcMethod {
    pub name: String,
    pub input_type: String,
    pub output_type: String,
    pub client_streaming: bool,
    pub server_streaming: bool,
}

impl GrpcMethod {
    pub fn method_type(&self) -> &'static str {
        match (self.client_streaming, self.server_streaming) {
            (false, false) => "Unary",
            (false, true) => "Server Streaming",
            (true, false) => "Client Streaming",
            (true, true) => "Bidirectional Streaming",
        }
    }
}

/// Parse a .proto file and extract service definitions
pub fn parse_proto_file(path: &Path) -> Result<Vec<GrpcService>> {
    let content = std::fs::read_to_string(path)?;
    parse_proto(&content)
}

/// Parse .proto content and extract services
pub fn parse_proto(content: &str) -> Result<Vec<GrpcService>> {
    let mut services = Vec::new();
    let mut current_service: Option<GrpcService> = None;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("service ") {
            // Start of a service definition
            let name = trimmed
                .strip_prefix("service ")
                .and_then(|s| s.split_whitespace().next())
                .unwrap_or("")
                .to_string();
            current_service = Some(GrpcService {
                name,
                methods: Vec::new(),
            });
        } else if trimmed.starts_with("rpc ") && current_service.is_some() {
            if let Some(method) = parse_rpc_line(trimmed) {
                current_service.as_mut().unwrap().methods.push(method);
            }
        } else if trimmed == "}" && current_service.is_some() {
            if let Some(service) = current_service.take() {
                services.push(service);
            }
        }
    }

    Ok(services)
}

fn parse_rpc_line(line: &str) -> Option<GrpcMethod> {
    // rpc MethodName (InputType) returns (OutputType);
    // rpc MethodName (stream InputType) returns (stream OutputType);
    let line = line.strip_prefix("rpc ")?.trim();

    let name_end = line.find('(')?;
    let name = line[..name_end].trim().to_string();

    let rest = &line[name_end..];

    // Parse input
    let input_start = rest.find('(')? + 1;
    let input_end = rest.find(')')?;
    let input_raw = rest[input_start..input_end].trim();
    let (client_streaming, input_type) = if input_raw.starts_with("stream ") {
        (true, input_raw.strip_prefix("stream ")?.trim().to_string())
    } else {
        (false, input_raw.to_string())
    };

    // Parse output
    let returns_pos = rest.find("returns")?;
    let output_part = &rest[returns_pos + 7..];
    let output_start = output_part.find('(')? + 1;
    let output_end = output_part.find(')')?;
    let output_raw = output_part[output_start..output_end].trim();
    let (server_streaming, output_type) = if output_raw.starts_with("stream ") {
        (true, output_raw.strip_prefix("stream ")?.trim().to_string())
    } else {
        (false, output_raw.to_string())
    };

    Some(GrpcMethod {
        name,
        input_type,
        output_type,
        client_streaming,
        server_streaming,
    })
}

/// Generate example JSON request body from a message type
pub fn generate_example_body(message_type: &str) -> String {
    // For now, return a basic template
    format!("{{\n  \"_type\": \"{}\"\n}}", message_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_proto() {
        let proto = r#"
syntax = "proto3";

package example;

service Greeter {
    rpc SayHello (HelloRequest) returns (HelloReply);
    rpc SayHelloStream (HelloRequest) returns (stream HelloReply);
    rpc ChatStream (stream ChatMessage) returns (stream ChatMessage);
}

message HelloRequest {
    string name = 1;
}

message HelloReply {
    string message = 1;
}
"#;
        let services = parse_proto(proto).unwrap();
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name, "Greeter");
        assert_eq!(services[0].methods.len(), 3);

        assert_eq!(services[0].methods[0].name, "SayHello");
        assert!(!services[0].methods[0].client_streaming);
        assert!(!services[0].methods[0].server_streaming);

        assert_eq!(services[0].methods[1].name, "SayHelloStream");
        assert!(!services[0].methods[1].client_streaming);
        assert!(services[0].methods[1].server_streaming);

        assert_eq!(services[0].methods[2].name, "ChatStream");
        assert!(services[0].methods[2].client_streaming);
        assert!(services[0].methods[2].server_streaming);
    }
}
