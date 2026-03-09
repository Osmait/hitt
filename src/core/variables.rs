use chrono::Utc;
use rand::Rng;
use regex::Regex;
use std::collections::HashMap;
use uuid::Uuid;

use super::environment::Environment;
use super::request::KeyValuePair;

pub struct VariableResolver {
    scopes: Vec<VariableScope>,
}

#[derive(Debug, Clone)]
pub struct VariableScope {
    pub name: String,
    pub variables: HashMap<String, String>,
}

impl VariableResolver {
    pub fn new() -> Self {
        Self {
            scopes: Vec::new(),
        }
    }

    /// Build a resolver with the standard scope chain:
    /// 1. Chain step extractions (runtime) — highest priority
    /// 2. Collection variables
    /// 3. Environment variables (active)
    /// 4. .env file variables
    /// 5. Global variables
    /// 6. Dynamic variables — lowest priority (handled at resolve time)
    pub fn from_context(
        chain_vars: Option<&HashMap<String, String>>,
        collection_vars: &[KeyValuePair],
        environment: Option<&Environment>,
        dotenv_vars: Option<&HashMap<String, String>>,
        global_vars: Option<&HashMap<String, String>>,
    ) -> Self {
        let mut resolver = Self::new();

        // Add scopes in priority order (first added = highest priority)
        if let Some(chain) = chain_vars {
            resolver.add_scope("chain", chain.clone());
        }

        let coll_map: HashMap<String, String> = collection_vars
            .iter()
            .filter(|kv| kv.enabled)
            .map(|kv| (kv.key.clone(), kv.value.clone()))
            .collect();
        if !coll_map.is_empty() {
            resolver.add_scope("collection", coll_map);
        }

        if let Some(env) = environment {
            let env_map: HashMap<String, String> = env
                .active_variables()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            if !env_map.is_empty() {
                resolver.add_scope("environment", env_map);
            }
        }

        if let Some(dotenv) = dotenv_vars {
            if !dotenv.is_empty() {
                resolver.add_scope("dotenv", dotenv.clone());
            }
        }

        if let Some(global) = global_vars {
            if !global.is_empty() {
                resolver.add_scope("global", global.clone());
            }
        }

        resolver
    }

    pub fn add_scope(&mut self, name: impl Into<String>, variables: HashMap<String, String>) {
        self.scopes.push(VariableScope {
            name: name.into(),
            variables,
        });
    }

    pub fn set_variable(&mut self, scope_name: &str, key: String, value: String) {
        if let Some(scope) = self.scopes.iter_mut().find(|s| s.name == scope_name) {
            scope.variables.insert(key, value);
        } else {
            let mut vars = HashMap::new();
            vars.insert(key, value);
            self.scopes.push(VariableScope {
                name: scope_name.to_string(),
                variables: vars,
            });
        }
    }

    pub fn resolve(&self, input: &str) -> String {
        let re = Regex::new(r"\{\{([^}]+)\}\}").unwrap();
        re.replace_all(input, |caps: &regex::Captures| {
            let var_name = caps[1].trim();
            self.resolve_variable(var_name)
                .unwrap_or_else(|| format!("{{{{{}}}}}", var_name))
        })
        .to_string()
    }

    fn resolve_variable(&self, name: &str) -> Option<String> {
        // Check dynamic variables first (prefixed with $)
        if name.starts_with('$') {
            return self.resolve_dynamic(name);
        }

        // Check scopes in priority order
        for scope in &self.scopes {
            if let Some(value) = scope.variables.get(name) {
                return Some(value.clone());
            }
        }

        None
    }

    fn resolve_dynamic(&self, name: &str) -> Option<String> {
        match name {
            "$timestamp" => Some(Utc::now().timestamp().to_string()),
            "$isoTimestamp" => Some(Utc::now().to_rfc3339()),
            "$randomInt" => {
                let mut rng = rand::thread_rng();
                Some(rng.gen_range(0..1000).to_string())
            }
            "$guid" => Some(Uuid::new_v4().to_string()),
            "$randomEmail" => {
                let mut rng = rand::thread_rng();
                let n: u32 = rng.gen_range(1000..9999);
                Some(format!("user{}@example.com", n))
            }
            "$randomFullName" => {
                let names = [
                    "John Doe",
                    "Jane Smith",
                    "Alice Johnson",
                    "Bob Williams",
                    "Charlie Brown",
                    "Diana Prince",
                    "Edward Norton",
                    "Fiona Apple",
                ];
                let mut rng = rand::thread_rng();
                let idx = rng.gen_range(0..names.len());
                Some(names[idx].to_string())
            }
            "$randomInt1000" => {
                let mut rng = rand::thread_rng();
                Some(rng.gen_range(0..1000).to_string())
            }
            "$randomBoolean" => {
                let mut rng = rand::thread_rng();
                Some(if rng.gen_bool(0.5) { "true" } else { "false" }.to_string())
            }
            _ => None,
        }
    }

    /// Resolve all variables in headers
    pub fn resolve_headers(&self, headers: &[KeyValuePair]) -> Vec<KeyValuePair> {
        headers
            .iter()
            .filter(|h| h.enabled)
            .map(|h| KeyValuePair {
                key: self.resolve(&h.key),
                value: self.resolve(&h.value),
                enabled: true,
                description: h.description.clone(),
            })
            .collect()
    }

    /// Resolve all variables in query params
    pub fn resolve_params(&self, params: &[KeyValuePair]) -> Vec<KeyValuePair> {
        params
            .iter()
            .filter(|p| p.enabled)
            .map(|p| KeyValuePair {
                key: self.resolve(&p.key),
                value: self.resolve(&p.value),
                enabled: true,
                description: p.description.clone(),
            })
            .collect()
    }

    /// Get all resolved variables for display
    pub fn all_variables(&self) -> Vec<(&str, &str, &str)> {
        let mut result = Vec::new();
        for scope in &self.scopes {
            for (k, v) in &scope.variables {
                result.push((scope.name.as_str(), k.as_str(), v.as_str()));
            }
        }
        result
    }
}

impl Default for VariableResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_interpolation() {
        let mut resolver = VariableResolver::new();
        let mut vars = HashMap::new();
        vars.insert("host".to_string(), "api.example.com".to_string());
        vars.insert("id".to_string(), "42".to_string());
        resolver.add_scope("env", vars);

        assert_eq!(
            resolver.resolve("https://{{host}}/users/{{id}}"),
            "https://api.example.com/users/42"
        );
    }

    #[test]
    fn test_unresolved_variable_preserved() {
        let resolver = VariableResolver::new();
        assert_eq!(
            resolver.resolve("{{unknown}}"),
            "{{unknown}}"
        );
    }

    #[test]
    fn test_dynamic_variables() {
        let resolver = VariableResolver::new();
        let result = resolver.resolve("{{$guid}}");
        assert_ne!(result, "{{$guid}}");
        assert!(Uuid::parse_str(&result).is_ok());
    }

    #[test]
    fn test_scope_priority() {
        let mut resolver = VariableResolver::new();
        let mut high = HashMap::new();
        high.insert("key".to_string(), "high".to_string());
        resolver.add_scope("high", high);

        let mut low = HashMap::new();
        low.insert("key".to_string(), "low".to_string());
        resolver.add_scope("low", low);

        assert_eq!(resolver.resolve("{{key}}"), "high");
    }
}
