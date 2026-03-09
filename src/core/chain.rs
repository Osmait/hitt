use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use super::response::Response;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestChain {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub steps: Vec<ChainStep>,
}

impl RequestChain {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: None,
            steps: Vec::new(),
        }
    }

    pub fn add_step(&mut self, request_id: Uuid) -> &mut ChainStep {
        self.steps.push(ChainStep {
            request_id,
            extractions: Vec::new(),
            condition: None,
            delay_ms: None,
        });
        self.steps.last_mut().unwrap()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainStep {
    pub request_id: Uuid,
    pub extractions: Vec<ValueExtraction>,
    pub condition: Option<StepCondition>,
    pub delay_ms: Option<u64>,
}

impl ChainStep {
    pub fn add_extraction(
        &mut self,
        source: ExtractionSource,
        json_path: impl Into<String>,
        variable_name: impl Into<String>,
    ) -> &mut Self {
        self.extractions.push(ValueExtraction {
            source,
            json_path: json_path.into(),
            variable_name: variable_name.into(),
        });
        self
    }

    pub fn with_condition(&mut self, condition: StepCondition) -> &mut Self {
        self.condition = Some(condition);
        self
    }

    pub fn with_delay(&mut self, delay_ms: u64) -> &mut Self {
        self.delay_ms = Some(delay_ms);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueExtraction {
    pub source: ExtractionSource,
    pub json_path: String,
    pub variable_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExtractionSource {
    Body,
    Header(String),
    Cookie(String),
    Status,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepCondition {
    StatusEquals(u16),
    StatusRange(u16, u16),
    BodyContains(String),
    VariableEquals(String, String),
    Always,
}

#[derive(Debug, Clone)]
pub enum ChainStepStatus {
    Pending,
    Running,
    Success { status: u16, duration_ms: u64 },
    Failed { error: String },
    Skipped { reason: String },
}

#[derive(Debug, Clone)]
pub struct ChainExecutionState {
    pub chain_id: Uuid,
    pub current_step: usize,
    pub step_statuses: Vec<ChainStepStatus>,
    pub extracted_variables: std::collections::HashMap<String, String>,
    pub running: bool,
}

impl ChainExecutionState {
    pub fn new(chain: &RequestChain) -> Self {
        Self {
            chain_id: chain.id,
            current_step: 0,
            step_statuses: vec![ChainStepStatus::Pending; chain.steps.len()],
            extracted_variables: HashMap::new(),
            running: false,
        }
    }
}

/// Extract values from a response based on extraction rules.
pub fn extract_values(
    extractions: &[ValueExtraction],
    response: &Response,
) -> HashMap<String, String> {
    use jsonpath_rust::JsonPath;

    let mut result = HashMap::new();
    // Parse body JSON once for all extractions that need it
    let body_json = response.body_json();

    for extraction in extractions {
        let value = match &extraction.source {
            ExtractionSource::Body => {
                if let Some(ref json) = body_json {
                    if let Ok(jsonpath) = extraction.json_path.parse::<JsonPath>() {
                        let found = jsonpath.find(json);
                        if found.is_null() {
                            None
                        } else if let serde_json::Value::Array(arr) = &found {
                            arr.first().map(json_value_to_string)
                        } else {
                            Some(json_value_to_string(&found))
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            ExtractionSource::Header(name) => response
                .header_value(name)
                .map(std::string::ToString::to_string),
            ExtractionSource::Cookie(name) => response
                .cookies
                .iter()
                .find(|c| c.name == *name)
                .map(|c| c.value.clone()),
            ExtractionSource::Status => Some(response.status.to_string()),
        };
        if let Some(val) = value {
            result.insert(extraction.variable_name.clone(), val);
        }
    }
    result
}

/// Evaluate a step condition against a response and current variables.
#[allow(clippy::implicit_hasher)]
pub fn evaluate_condition(
    condition: &StepCondition,
    response: Option<&Response>,
    variables: &HashMap<String, String>,
) -> bool {
    match condition {
        StepCondition::Always => true,
        StepCondition::StatusEquals(code) => response.is_some_and(|r| r.status == *code),
        StepCondition::StatusRange(lo, hi) => {
            response.is_some_and(|r| r.status >= *lo && r.status <= *hi)
        }
        StepCondition::BodyContains(s) => response
            .and_then(|r| r.body_text())
            .is_some_and(|body| body.contains(s.as_str())),
        StepCondition::VariableEquals(name, val) => variables.get(name).is_some_and(|v| v == val),
    }
}

fn json_value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
