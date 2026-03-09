use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub id: Uuid,
    pub name: String,
    pub values: Vec<EnvironmentVariable>,
}

impl Environment {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            values: Vec::new(),
        }
    }

    pub fn add_variable(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> &mut Self {
        let val = value.into();
        self.values.push(EnvironmentVariable {
            key: key.into(),
            value: val.clone(),
            initial_value: val,
            enabled: true,
            secret: false,
        });
        self
    }

    pub fn add_secret(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> &mut Self {
        let val = value.into();
        self.values.push(EnvironmentVariable {
            key: key.into(),
            value: val.clone(),
            initial_value: val,
            enabled: true,
            secret: true,
        });
        self
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|v| v.enabled && v.key == key)
            .map(|v| v.value.as_str())
    }

    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        if let Some(var) = self.values.iter_mut().find(|v| v.key == key) {
            var.value = value.into();
        } else {
            self.add_variable(key, value.into());
        }
    }

    pub fn active_variables(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values
            .iter()
            .filter(|v| v.enabled)
            .map(|v| (v.key.as_str(), v.value.as_str()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentVariable {
    pub key: String,
    pub value: String,
    pub initial_value: String,
    pub enabled: bool,
    pub secret: bool,
}
