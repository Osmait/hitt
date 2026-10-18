use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthConfig {
    Bearer {
        token: String,
    },
    Basic {
        username: String,
        password: String,
    },
    ApiKey {
        key: String,
        value: String,
        location: ApiKeyLocation,
    },
    OAuth2 {
        grant_type: OAuth2GrantType,
        access_token_url: String,
        client_id: String,
        client_secret: String,
        scope: Option<String>,
        token: Option<String>,
    },
    Inherit,
    None,
}

impl AuthConfig {
    pub fn bearer(token: impl Into<String>) -> Self {
        Self::Bearer {
            token: token.into(),
        }
    }

    pub fn basic(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self::Basic {
            username: username.into(),
            password: password.into(),
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Bearer { .. } => "Bearer Token",
            Self::Basic { .. } => "Basic Auth",
            Self::ApiKey { .. } => "API Key",
            Self::OAuth2 { .. } => "OAuth 2.0",
            Self::Inherit => "Inherit from parent",
            Self::None => "No Auth",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ApiKeyLocation {
    Header,
    QueryParam,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OAuth2GrantType {
    AuthorizationCode,
    ClientCredentials,
    PasswordCredentials,
    Implicit,
}

impl OAuth2GrantType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AuthorizationCode => "authorization_code",
            Self::ClientCredentials => "client_credentials",
            Self::PasswordCredentials => "password",
            Self::Implicit => "implicit",
        }
    }
}
