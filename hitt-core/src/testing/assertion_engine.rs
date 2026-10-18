use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

use crate::core::response::Response;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assertion {
    pub id: Uuid,
    pub kind: AssertionKind,
    pub enabled: bool,
}

impl Assertion {
    pub fn new(kind: AssertionKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            enabled: true,
        }
    }

    pub fn status_equals(status: u16) -> Self {
        Self::new(AssertionKind::StatusEquals(status))
    }

    pub fn status_range(min: u16, max: u16) -> Self {
        Self::new(AssertionKind::StatusRange(min, max))
    }

    pub fn body_contains(substring: impl Into<String>) -> Self {
        Self::new(AssertionKind::BodyContains(substring.into()))
    }

    pub fn header_exists(name: impl Into<String>) -> Self {
        Self::new(AssertionKind::HeaderExists(name.into()))
    }

    pub fn response_time_less_than(ms: u64) -> Self {
        Self::new(AssertionKind::ResponseTimeLessThan(Duration::from_millis(
            ms,
        )))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AssertionKind {
    StatusEquals(u16),
    StatusRange(u16, u16),
    BodyPathEquals {
        path: String,
        expected: serde_json::Value,
    },
    BodyPathExists(String),
    BodyPathType {
        path: String,
        expected: JsonType,
    },
    BodyPathContains {
        path: String,
        substring: String,
    },
    BodyContains(String),
    HeaderEquals {
        name: String,
        expected: String,
    },
    HeaderExists(String),
    ResponseTimeLessThan(Duration),
    SizeLessThan(usize),
    MatchesJsonSchema(serde_json::Value),
}

impl AssertionKind {
    pub fn description(&self) -> String {
        match self {
            Self::StatusEquals(s) => format!("status == {s}"),
            Self::StatusRange(min, max) => format!("status in {min}..{max}"),
            Self::BodyPathEquals { path, expected } => format!("{path} == {expected}"),
            Self::BodyPathExists(path) => format!("{path} exists"),
            Self::BodyPathType { path, expected } => format!("{path} is {expected:?}"),
            Self::BodyPathContains { path, substring } => {
                format!("{path} contains \"{substring}\"")
            }
            Self::BodyContains(s) => format!("body contains \"{s}\""),
            Self::HeaderEquals { name, expected } => format!("header {name} == \"{expected}\""),
            Self::HeaderExists(name) => format!("header {name} exists"),
            Self::ResponseTimeLessThan(d) => format!("time < {}ms", d.as_millis()),
            Self::SizeLessThan(s) => format!("size < {s} bytes"),
            Self::MatchesJsonSchema(_) => "matches JSON schema".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JsonType {
    String,
    Number,
    Boolean,
    Array,
    Object,
    Null,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssertionResult {
    pub assertion: Assertion,
    pub passed: bool,
    pub actual_value: Option<String>,
    pub message: String,
}

pub struct AssertionEngine;

impl AssertionEngine {
    pub fn run_assertions(assertions: &[Assertion], response: &Response) -> Vec<AssertionResult> {
        assertions
            .iter()
            .filter(|a| a.enabled)
            .map(|a| Self::evaluate(a, response))
            .collect()
    }

    pub fn evaluate(assertion: &Assertion, response: &Response) -> AssertionResult {
        match &assertion.kind {
            AssertionKind::StatusEquals(expected) => {
                let passed = response.status == *expected;
                AssertionResult {
                    assertion: assertion.clone(),
                    passed,
                    actual_value: Some(response.status.to_string()),
                    message: if passed {
                        format!("Status is {expected}")
                    } else {
                        format!("Expected status {}, got {}", expected, response.status)
                    },
                }
            }

            AssertionKind::StatusRange(min, max) => {
                let passed = response.status >= *min && response.status <= *max;
                AssertionResult {
                    assertion: assertion.clone(),
                    passed,
                    actual_value: Some(response.status.to_string()),
                    message: if passed {
                        format!("Status {} is in range {}..{}", response.status, min, max)
                    } else {
                        format!(
                            "Expected status in {}..{}, got {}",
                            min, max, response.status
                        )
                    },
                }
            }

            AssertionKind::BodyContains(substring) => {
                let body = response.body_text().unwrap_or("");
                let passed = body.contains(substring.as_str());
                AssertionResult {
                    assertion: assertion.clone(),
                    passed,
                    actual_value: None,
                    message: if passed {
                        format!("Body contains \"{substring}\"")
                    } else {
                        format!("Body does not contain \"{substring}\"")
                    },
                }
            }

            AssertionKind::BodyPathExists(path) => {
                let result = eval_jsonpath(response, path);
                let passed = result.is_some();
                AssertionResult {
                    assertion: assertion.clone(),
                    passed,
                    actual_value: result.map(|v| v.to_string()),
                    message: if passed {
                        format!("{path} exists")
                    } else {
                        format!("{path} does not exist")
                    },
                }
            }

            AssertionKind::BodyPathEquals { path, expected } => {
                let result = eval_jsonpath(response, path);
                let passed = result.as_ref().is_some_and(|v| v == expected);
                let actual_str = result.as_ref().map(std::string::ToString::to_string);
                let message = if passed {
                    format!("{path} == {expected}")
                } else {
                    format!(
                        "{} != {} (expected {})",
                        path,
                        actual_str.as_deref().unwrap_or("null"),
                        expected
                    )
                };
                AssertionResult {
                    assertion: assertion.clone(),
                    passed,
                    actual_value: actual_str,
                    message,
                }
            }

            AssertionKind::BodyPathType { path, expected } => {
                let result = eval_jsonpath(response, path);
                let actual_type = result.as_ref().map(json_type_of);
                let expected_str = format!("{expected:?}").to_lowercase();
                let passed = actual_type.as_ref().is_some_and(|t| t == &expected_str);
                AssertionResult {
                    assertion: assertion.clone(),
                    passed,
                    actual_value: actual_type.clone(),
                    message: if passed {
                        format!("{path} is {expected_str}")
                    } else {
                        format!(
                            "{} is {} (expected {})",
                            path,
                            actual_type.unwrap_or_else(|| "absent".into()),
                            expected_str
                        )
                    },
                }
            }

            AssertionKind::BodyPathContains { path, substring } => {
                let result = eval_jsonpath(response, path);
                let passed = result
                    .as_ref()
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| s.contains(substring.as_str()));
                AssertionResult {
                    assertion: assertion.clone(),
                    passed,
                    actual_value: result.map(|v| v.to_string()),
                    message: if passed {
                        format!("{path} contains \"{substring}\"")
                    } else {
                        format!("{path} does not contain \"{substring}\"")
                    },
                }
            }

            AssertionKind::HeaderEquals { name, expected } => {
                let actual = response.header_value(name);
                let passed = actual.is_some_and(|v| v == expected.as_str());
                AssertionResult {
                    assertion: assertion.clone(),
                    passed,
                    actual_value: actual.map(std::string::ToString::to_string),
                    message: if passed {
                        format!("Header {name} == \"{expected}\"")
                    } else {
                        format!(
                            "Header {} is \"{}\" (expected \"{}\")",
                            name,
                            actual.unwrap_or("absent"),
                            expected
                        )
                    },
                }
            }

            AssertionKind::HeaderExists(name) => {
                let actual = response.header_value(name);
                let passed = actual.is_some();
                AssertionResult {
                    assertion: assertion.clone(),
                    passed,
                    actual_value: actual.map(std::string::ToString::to_string),
                    message: if passed {
                        format!("Header {name} exists")
                    } else {
                        format!("Header {name} does not exist")
                    },
                }
            }

            AssertionKind::ResponseTimeLessThan(max_duration) => {
                let actual = response.timing.total;
                let passed = actual < *max_duration;
                AssertionResult {
                    assertion: assertion.clone(),
                    passed,
                    actual_value: Some(format!("{}ms", actual.as_millis())),
                    message: if passed {
                        format!(
                            "Response time {}ms < {}ms",
                            actual.as_millis(),
                            max_duration.as_millis()
                        )
                    } else {
                        format!(
                            "Response time {}ms >= {}ms",
                            actual.as_millis(),
                            max_duration.as_millis()
                        )
                    },
                }
            }

            AssertionKind::SizeLessThan(max_size) => {
                let actual = response.size.total();
                let passed = actual < *max_size;
                AssertionResult {
                    assertion: assertion.clone(),
                    passed,
                    actual_value: Some(format!("{actual} bytes")),
                    message: if passed {
                        format!("Size {actual} < {max_size}")
                    } else {
                        format!("Size {actual} >= {max_size}")
                    },
                }
            }

            AssertionKind::MatchesJsonSchema(schema) => {
                let body_json = response.body_json();
                match body_json {
                    Some(value) => {
                        let result = crate::testing::schema_validator::validate(&value, schema);
                        AssertionResult {
                            assertion: assertion.clone(),
                            passed: result.is_ok(),
                            actual_value: None,
                            message: match result {
                                Ok(()) => "Response matches JSON schema".to_string(),
                                Err(errors) => {
                                    format!("Schema validation failed: {}", errors.join(", "))
                                }
                            },
                        }
                    }
                    None => AssertionResult {
                        assertion: assertion.clone(),
                        passed: false,
                        actual_value: None,
                        message: "Response body is not valid JSON".to_string(),
                    },
                }
            }
        }
    }

    pub fn summary(results: &[AssertionResult]) -> (usize, usize) {
        let passed = results.iter().filter(|r| r.passed).count();
        (passed, results.len())
    }
}

fn eval_jsonpath(response: &Response, path: &str) -> Option<serde_json::Value> {
    use jsonpath_rust::JsonPath;
    let json = response.body_json()?;
    let jsonpath: JsonPath = path.parse().ok()?;
    let result = jsonpath.find(&json);
    // find returns a single Value; if it's Null, treat as absent
    if result.is_null() {
        None
    } else if let serde_json::Value::Array(arr) = &result {
        arr.first().cloned()
    } else {
        Some(result)
    }
}

fn json_type_of(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(_) => "boolean".to_string(),
        serde_json::Value::Number(_) => "number".to_string(),
        serde_json::Value::String(_) => "string".to_string(),
        serde_json::Value::Array(_) => "array".to_string(),
        serde_json::Value::Object(_) => "object".to_string(),
    }
}
