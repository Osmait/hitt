use anyhow::Result;

use super::schema_v2_1::{PostmanEnvValue, PostmanEnvironment};
use crate::core::environment::Environment;

pub fn export_postman_environment(env: &Environment) -> Result<String> {
    let postman_env = PostmanEnvironment {
        id: env.id.to_string(),
        name: env.name.clone(),
        values: env
            .values
            .iter()
            .map(|v| PostmanEnvValue {
                key: v.key.clone(),
                value: if v.secret {
                    String::new() // Don't export secret values
                } else {
                    v.value.clone()
                },
                value_type: if v.secret {
                    Some("secret".to_string())
                } else {
                    Some("default".to_string())
                },
                enabled: v.enabled,
            })
            .collect(),
        scope: Some("environment".to_string()),
    };

    let json = serde_json::to_string_pretty(&postman_env)?;
    Ok(json)
}
