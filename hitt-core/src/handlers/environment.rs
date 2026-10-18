use crate::core::environment::Environment;
use crate::{Core, CoreEvent};

pub(crate) async fn load(core: &mut Core) -> Vec<CoreEvent> {
    let store = match core.collections_store.as_ref() {
        Some(s) => s,
        None => return vec![CoreEvent::Error("Collections store not initialized".into())],
    };
    match store.load_environments() {
        Ok(list) => {
            core.environments = list.clone();
            vec![CoreEvent::Environments(list)]
        }
        Err(e) => vec![CoreEvent::Error(e.to_string())],
    }
}

pub(crate) async fn save(core: &mut Core, env: Environment) -> Vec<CoreEvent> {
    let store = match core.collections_store.as_ref() {
        Some(s) => s,
        None => return vec![CoreEvent::Error("Collections store not initialized".into())],
    };
    if let Err(e) = store.save_environment(&env) {
        return vec![CoreEvent::Error(e.to_string())];
    }
    match core.environments.iter().position(|e| e.name == env.name) {
        Some(idx) => core.environments[idx] = env.clone(),
        None => core.environments.push(env.clone()),
    }
    vec![CoreEvent::EnvironmentSaved(env)]
}
