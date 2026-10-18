use serde_json::Value;

pub fn validate(instance: &Value, schema: &Value) -> Result<(), Vec<String>> {
    let validator = jsonschema::validator_for(schema).map_err(|e| vec![e.to_string()])?;

    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|e| {
            let path = e.instance_path.to_string();
            if path.is_empty() {
                e.to_string()
            } else {
                format!("{path}: {e}")
            }
        })
        .collect();

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
