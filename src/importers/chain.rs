use anyhow::{bail, Result};
use serde::Deserialize;

use crate::core::chain::{
    ChainStep, ExtractionSource, RequestChain, StepCondition, ValueExtraction,
};
use crate::core::collection::Collection;

#[derive(Debug, Deserialize)]
struct ChainFile {
    name: String,
    description: Option<String>,
    steps: Vec<ChainFileStep>,
}

#[derive(Debug, Deserialize)]
struct ChainFileStep {
    request: String,
    extract: Option<Vec<ChainFileExtraction>>,
    condition: Option<ChainFileCondition>,
    delay_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ChainFileExtraction {
    source: String,
    path: Option<String>,
    name: Option<String>,
    variable: String,
}

#[derive(Debug, Deserialize)]
struct ChainFileCondition {
    #[serde(rename = "type")]
    kind: String,
    value: Option<serde_yaml::Value>,
    from: Option<u16>,
    to: Option<u16>,
    name: Option<String>,
}

/// Import a chain definition from YAML content, resolving request names against the collection.
pub fn import_chain(content: &str, collection: &Collection) -> Result<RequestChain> {
    let file: ChainFile = serde_yaml::from_str(content)?;

    let all_requests = collection.all_requests();

    let mut chain = RequestChain::new(&file.name);
    chain.description = file.description;

    let mut not_found = Vec::new();

    for file_step in &file.steps {
        let request = all_requests.iter().find(|r| r.name == file_step.request);

        let request_id = match request {
            Some(r) => r.id,
            None => {
                not_found.push(file_step.request.clone());
                continue;
            }
        };

        let step = chain.add_step(request_id);

        if let Some(delay) = file_step.delay_ms {
            step.with_delay(delay);
        }

        if let Some(condition) = &file_step.condition {
            step.with_condition(map_condition(condition)?);
        }

        if let Some(extractions) = &file_step.extract {
            for ext in extractions {
                let (source, json_path) = map_extraction_source(ext)?;
                step.add_extraction(source, json_path, &ext.variable);
            }
        }
    }

    if !not_found.is_empty() {
        bail!(
            "Request(s) not found in collection '{}': {}",
            collection.name,
            not_found.join(", ")
        );
    }

    Ok(chain)
}

fn map_condition(cond: &ChainFileCondition) -> Result<StepCondition> {
    match cond.kind.as_str() {
        "always" => Ok(StepCondition::Always),
        "status_equals" => {
            let value = cond
                .value
                .as_ref()
                .and_then(|v| v.as_u64())
                .map(|v| v as u16);
            match value {
                Some(code) => Ok(StepCondition::StatusEquals(code)),
                None => bail!("status_equals condition requires a numeric 'value'"),
            }
        }
        "status_range" => match (cond.from, cond.to) {
            (Some(from), Some(to)) => Ok(StepCondition::StatusRange(from, to)),
            _ => bail!("status_range condition requires 'from' and 'to'"),
        },
        "body_contains" => {
            let value = cond
                .value
                .as_ref()
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            match value {
                Some(s) => Ok(StepCondition::BodyContains(s)),
                None => bail!("body_contains condition requires a string 'value'"),
            }
        }
        "variable_equals" => match (&cond.name, &cond.value) {
            (Some(name), Some(value)) => {
                let val_str = value
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("{}", value.as_u64().unwrap_or(0)));
                Ok(StepCondition::VariableEquals(name.clone(), val_str))
            }
            _ => bail!("variable_equals condition requires 'name' and 'value'"),
        },
        other => bail!("Unknown condition type: '{}'", other),
    }
}

fn map_extraction_source(ext: &ChainFileExtraction) -> Result<(ExtractionSource, String)> {
    match ext.source.as_str() {
        "body" => {
            let path = ext.path.clone().unwrap_or_default();
            Ok((ExtractionSource::Body, path))
        }
        "header" => match &ext.name {
            Some(name) => Ok((ExtractionSource::Header(name.clone()), String::new())),
            None => bail!("header extraction requires 'name'"),
        },
        "cookie" => match &ext.name {
            Some(name) => Ok((ExtractionSource::Cookie(name.clone()), String::new())),
            None => bail!("cookie extraction requires 'name'"),
        },
        "status" => Ok((ExtractionSource::Status, String::new())),
        other => bail!("Unknown extraction source: '{}'", other),
    }
}

/// Returns true if the YAML content looks like a chain definition (has a top-level `steps` key).
pub fn looks_like_chain(content: &str) -> bool {
    // Quick check: try to parse as a generic YAML mapping and look for "steps"
    if let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(content) {
        if let Some(map) = value.as_mapping() {
            return map.contains_key(&serde_yaml::Value::String("steps".into()));
        }
    }
    false
}
