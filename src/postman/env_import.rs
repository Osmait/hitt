use anyhow::Result;
use uuid::Uuid;

use super::schema_v2_1::PostmanEnvironment;
use crate::core::environment::{Environment, EnvironmentVariable};

pub fn import_postman_environment(content: &str) -> Result<Environment> {
    let postman_env: PostmanEnvironment = serde_json::from_str(content)?;

    let mut env = Environment::new(&postman_env.name);
    env.id = Uuid::parse_str(&postman_env.id).unwrap_or_else(|_| Uuid::new_v4());

    env.values = postman_env
        .values
        .iter()
        .map(|v| {
            let is_secret = v.value_type.as_deref().is_some_and(|t| t == "secret");
            EnvironmentVariable {
                key: v.key.clone(),
                value: v.value.clone(),
                initial_value: v.value.clone(),
                enabled: v.enabled,
                secret: is_secret,
            }
        })
        .collect();

    Ok(env)
}
