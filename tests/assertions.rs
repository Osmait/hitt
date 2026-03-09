use std::time::Duration;
use uuid::Uuid;

use hitt::core::request::KeyValuePair;
use hitt::core::response::{RequestTiming, Response, ResponseBody, ResponseSize};
use hitt::testing::assertion_engine::{Assertion, AssertionEngine, AssertionKind, JsonType};

fn make_response(status: u16, body: ResponseBody, headers: Vec<KeyValuePair>) -> Response {
    Response {
        id: Uuid::new_v4(),
        status,
        status_text: String::new(),
        headers,
        body,
        cookies: vec![],
        timing: RequestTiming::simple(Duration::from_millis(150)),
        size: ResponseSize {
            headers: 200,
            body: 500,
        },
        assertion_results: vec![],
        timestamp: chrono::Utc::now(),
    }
}

fn json_response(status: u16, json: &str) -> Response {
    make_response(
        status,
        ResponseBody::Json(json.to_string()),
        vec![KeyValuePair::new("Content-Type", "application/json")],
    )
}

// ── StatusEquals ────────────────────────────────────────────────────────────

#[test]
fn status_equals_pass() {
    let resp = json_response(200, "{}");
    let a = Assertion::status_equals(200);
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(result.passed);
}

#[test]
fn status_equals_fail() {
    let resp = json_response(404, "{}");
    let a = Assertion::status_equals(200);
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(!result.passed);
    assert_eq!(result.actual_value, Some("404".into()));
}

// ── StatusRange ─────────────────────────────────────────────────────────────

#[test]
fn status_range_pass() {
    let resp = json_response(201, "{}");
    let a = Assertion::status_range(200, 299);
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(result.passed);
}

#[test]
fn status_range_fail() {
    let resp = json_response(500, "{}");
    let a = Assertion::status_range(200, 299);
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(!result.passed);
}

// ── BodyContains ────────────────────────────────────────────────────────────

#[test]
fn body_contains_pass() {
    let resp = json_response(200, r#"{"message":"hello world"}"#);
    let a = Assertion::body_contains("hello");
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(result.passed);
}

#[test]
fn body_contains_fail() {
    let resp = json_response(200, r#"{"message":"hi"}"#);
    let a = Assertion::body_contains("hello");
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(!result.passed);
}

// ── BodyPathExists ──────────────────────────────────────────────────────────

#[test]
fn body_path_exists_pass() {
    let resp = json_response(200, r#"{"user":{"name":"John"}}"#);
    let a = Assertion::new(AssertionKind::BodyPathExists("$.user.name".into()));
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(result.passed);
}

#[test]
fn body_path_exists_fail() {
    let resp = json_response(200, r#"{"user":{}}"#);
    let a = Assertion::new(AssertionKind::BodyPathExists("$.user.email".into()));
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(!result.passed);
}

// ── BodyPathEquals ──────────────────────────────────────────────────────────

#[test]
fn body_path_equals_pass() {
    let resp = json_response(200, r#"{"count":42}"#);
    let a = Assertion::new(AssertionKind::BodyPathEquals {
        path: "$.count".into(),
        expected: serde_json::json!(42),
    });
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(result.passed);
}

#[test]
fn body_path_equals_fail() {
    let resp = json_response(200, r#"{"count":10}"#);
    let a = Assertion::new(AssertionKind::BodyPathEquals {
        path: "$.count".into(),
        expected: serde_json::json!(42),
    });
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(!result.passed);
}

// ── BodyPathType ────────────────────────────────────────────────────────────

#[test]
fn body_path_type_string() {
    let resp = json_response(200, r#"{"name":"John"}"#);
    let a = Assertion::new(AssertionKind::BodyPathType {
        path: "$.name".into(),
        expected: JsonType::String,
    });
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(result.passed);
}

#[test]
fn body_path_type_number() {
    let resp = json_response(200, r#"{"age":30}"#);
    let a = Assertion::new(AssertionKind::BodyPathType {
        path: "$.age".into(),
        expected: JsonType::Number,
    });
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(result.passed);
}

#[test]
fn body_path_type_boolean() {
    let resp = json_response(200, r#"{"active":true}"#);
    let a = Assertion::new(AssertionKind::BodyPathType {
        path: "$.active".into(),
        expected: JsonType::Boolean,
    });
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(result.passed);
}

// ── BodyPathContains ────────────────────────────────────────────────────────

#[test]
fn body_path_contains_pass() {
    let resp = json_response(200, r#"{"greeting":"hello world"}"#);
    let a = Assertion::new(AssertionKind::BodyPathContains {
        path: "$.greeting".into(),
        substring: "world".into(),
    });
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(result.passed);
}

#[test]
fn body_path_contains_fail() {
    let resp = json_response(200, r#"{"greeting":"hello"}"#);
    let a = Assertion::new(AssertionKind::BodyPathContains {
        path: "$.greeting".into(),
        substring: "world".into(),
    });
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(!result.passed);
}

// ── HeaderEquals ────────────────────────────────────────────────────────────

#[test]
fn header_equals_pass() {
    let resp = make_response(
        200,
        ResponseBody::Empty,
        vec![KeyValuePair::new("X-Custom", "hello")],
    );
    let a = Assertion::new(AssertionKind::HeaderEquals {
        name: "X-Custom".into(),
        expected: "hello".into(),
    });
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(result.passed);
}

#[test]
fn header_equals_case_insensitive_name() {
    let resp = make_response(
        200,
        ResponseBody::Empty,
        vec![KeyValuePair::new("Content-Type", "text/html")],
    );
    let a = Assertion::new(AssertionKind::HeaderEquals {
        name: "content-type".into(),
        expected: "text/html".into(),
    });
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(result.passed);
}

// ── HeaderExists ────────────────────────────────────────────────────────────

#[test]
fn header_exists_pass() {
    let resp = make_response(
        200,
        ResponseBody::Empty,
        vec![KeyValuePair::new("X-Request-Id", "abc")],
    );
    let a = Assertion::header_exists("X-Request-Id");
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(result.passed);
}

#[test]
fn header_exists_fail() {
    let resp = make_response(200, ResponseBody::Empty, vec![]);
    let a = Assertion::header_exists("X-Missing");
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(!result.passed);
}

// ── ResponseTimeLessThan ────────────────────────────────────────────────────

#[test]
fn response_time_pass() {
    let resp = json_response(200, "{}"); // timing: 150ms
    let a = Assertion::response_time_less_than(500);
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(result.passed);
}

#[test]
fn response_time_fail() {
    let resp = json_response(200, "{}"); // timing: 150ms
    let a = Assertion::response_time_less_than(100);
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(!result.passed);
}

// ── SizeLessThan ────────────────────────────────────────────────────────────

#[test]
fn size_less_than_pass() {
    let resp = json_response(200, "{}"); // size: 200 + 500 = 700
    let a = Assertion::new(AssertionKind::SizeLessThan(1000));
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(result.passed);
}

#[test]
fn size_less_than_fail() {
    let resp = json_response(200, "{}"); // size: 700
    let a = Assertion::new(AssertionKind::SizeLessThan(500));
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(!result.passed);
}

// ── MatchesJsonSchema ───────────────────────────────────────────────────────

#[test]
fn json_schema_valid() {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "age": { "type": "integer" }
        },
        "required": ["name"]
    });
    let resp = json_response(200, r#"{"name":"John","age":30}"#);
    let a = Assertion::new(AssertionKind::MatchesJsonSchema(schema));
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(result.passed);
}

#[test]
fn json_schema_invalid() {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" }
        },
        "required": ["name"]
    });
    let resp = json_response(200, r#"{"age":30}"#);
    let a = Assertion::new(AssertionKind::MatchesJsonSchema(schema));
    let result = AssertionEngine::evaluate(&a, &resp);
    assert!(!result.passed);
}

// ── run_assertions & summary ────────────────────────────────────────────────

#[test]
fn run_assertions_and_summary() {
    let resp = json_response(200, r#"{"ok":true}"#);
    let assertions = vec![
        Assertion::status_equals(200),
        Assertion::body_contains("ok"),
        Assertion::status_equals(404), // will fail
    ];
    let results = AssertionEngine::run_assertions(&assertions, &resp);
    assert_eq!(results.len(), 3);

    let (passed, total) = AssertionEngine::summary(&results);
    assert_eq!(passed, 2);
    assert_eq!(total, 3);
}

#[test]
fn disabled_assertions_skipped() {
    let resp = json_response(200, "{}");
    let mut disabled = Assertion::status_equals(404);
    disabled.enabled = false;

    let assertions = vec![Assertion::status_equals(200), disabled];
    let results = AssertionEngine::run_assertions(&assertions, &resp);
    // Only enabled assertions are run
    assert_eq!(results.len(), 1);
    assert!(results[0].passed);
}
