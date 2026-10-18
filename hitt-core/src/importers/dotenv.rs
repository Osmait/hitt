use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::core::environment::Environment;

pub fn parse_dotenv(content: &str) -> Result<HashMap<String, String>> {
    let mut vars = HashMap::new();

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = parse_line(line) {
            vars.insert(key, value);
        }
    }

    Ok(vars)
}

pub fn parse_dotenv_file(path: &Path) -> Result<HashMap<String, String>> {
    let content = std::fs::read_to_string(path)?;
    parse_dotenv(&content)
}

pub fn dotenv_to_environment(name: impl Into<String>, content: &str) -> Result<Environment> {
    let vars = parse_dotenv(content)?;
    let mut env = Environment::new(name);

    for (key, value) in vars {
        env.add_variable(key, value);
    }

    Ok(env)
}

fn parse_line(line: &str) -> Option<(String, String)> {
    // Handle export prefix
    let line = line.strip_prefix("export ").unwrap_or(line);

    let (key, value) = line.split_once('=')?;
    let key = key.trim().to_string();

    if key.is_empty() {
        return None;
    }

    let value = value.trim();
    let value = unquote(value);

    Some((key, value))
}

fn unquote(s: &str) -> String {
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        let inner = &s[1..s.len() - 1];
        // Handle escape sequences in double-quoted strings
        if s.starts_with('"') {
            inner
                .replace("\\n", "\n")
                .replace("\\r", "\r")
                .replace("\\t", "\t")
                .replace("\\\"", "\"")
                .replace("\\\\", "\\")
        } else {
            inner.to_string()
        }
    } else {
        // Remove inline comments for unquoted values
        s.split('#').next().unwrap_or(s).trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_parsing() {
        let content = "KEY=value\nNAME=John";
        let vars = parse_dotenv(content).unwrap();
        assert_eq!(vars.get("KEY").unwrap(), "value");
        assert_eq!(vars.get("NAME").unwrap(), "John");
    }

    #[test]
    fn test_quoted_values() {
        let content = r#"
KEY1="hello world"
KEY2='single quoted'
KEY3=unquoted
"#;
        let vars = parse_dotenv(content).unwrap();
        assert_eq!(vars.get("KEY1").unwrap(), "hello world");
        assert_eq!(vars.get("KEY2").unwrap(), "single quoted");
        assert_eq!(vars.get("KEY3").unwrap(), "unquoted");
    }

    #[test]
    fn test_comments() {
        let content = "# This is a comment\nKEY=value # inline comment\n";
        let vars = parse_dotenv(content).unwrap();
        assert_eq!(vars.get("KEY").unwrap(), "value");
        assert_eq!(vars.len(), 1);
    }

    #[test]
    fn test_export_prefix() {
        let content = "export API_KEY=abc123";
        let vars = parse_dotenv(content).unwrap();
        assert_eq!(vars.get("API_KEY").unwrap(), "abc123");
    }

    #[test]
    fn test_escape_sequences() {
        let content = r#"MSG="hello\nworld""#;
        let vars = parse_dotenv(content).unwrap();
        assert_eq!(vars.get("MSG").unwrap(), "hello\nworld");
    }
}
