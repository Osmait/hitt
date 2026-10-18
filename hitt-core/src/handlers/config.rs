use crate::core::client::HttpClient;
use crate::storage::config::AppConfig;
use crate::{Core, CoreEvent};

pub(crate) async fn save(core: &mut Core, new_config: AppConfig) -> Vec<CoreEvent> {
    if let Err(e) = new_config.save() {
        return vec![CoreEvent::Error(e.to_string())];
    }
    // Rebuild the client so timeout / redirect / SSL changes take effect.
    match HttpClient::from_config(&new_config) {
        Ok(client) => core.client = client,
        Err(e) => return vec![CoreEvent::Error(e.to_string())],
    }
    core.config = new_config;
    vec![CoreEvent::ConfigSaved]
}
