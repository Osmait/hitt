//! Renaming and deleting environments (`:renameenv`, `:delenv`, `R` / `D` in
//! the environment modal).

use hitt_core::storage::collections_store::CollectionsStore;

use crate::app::{App, AppMode, ConfirmAction, ModalKind, NotificationKind};

use super::actions::open_confirm;
use super::persistence::save_environment;

/// Rename the environment at `idx`. A saved environment's old file is deleted
/// before it is written under the new name. Errors are user-facing messages.
pub(super) fn rename_environment(app: &mut App, idx: usize, new_name: &str) -> Result<(), String> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err("Name is required".into());
    }
    let Some(env) = app.environments.get(idx) else {
        return Err("No active environment".into());
    };
    let old_name = env.name.clone();
    if old_name == new_name {
        return Ok(());
    }
    if app
        .environments
        .iter()
        .enumerate()
        .any(|(i, e)| i != idx && e.name == new_name)
    {
        return Err(format!("Environment '{new_name}' already exists"));
    }

    let session_only = app.session_env_ids.contains(&env.id);
    if !session_only {
        match CollectionsStore::new(app.config.collections_dir.clone()) {
            Ok(store) => {
                if let Err(e) = store.delete_environment(env) {
                    return Err(format!("Failed to remove '{old_name}': {e}"));
                }
            }
            Err(e) => return Err(format!("Failed to open environments store: {e}")),
        }
    }
    app.environments[idx].name = new_name.to_string();
    if !session_only {
        save_environment(app, idx);
    }
    if app.active_env == Some(idx) {
        app.remember_active_env();
    }
    app.notify(
        format!("Renamed environment '{old_name}' to '{new_name}'"),
        NotificationKind::Success,
    );
    Ok(())
}

/// Ask for confirmation before deleting the environment at `idx`.
pub(super) fn request_delete_environment(app: &mut App, idx: usize) {
    let Some(env) = app.environments.get(idx) else {
        return;
    };
    let message = format!("Delete environment '{}'?", env.name);
    let id = env.id;
    // Asked from the environment editor: return there whatever the answer.
    let return_mode =
        (app.mode == AppMode::Modal(ModalKind::EnvironmentEdit)).then(|| app.mode.clone());
    open_confirm(app, message, ConfirmAction::DeleteEnvironment(id));
    app.confirm_return_mode = return_mode;
}

/// Delete the environment with `id` from memory and disk, keeping
/// `active_env` pointing at the same environment (or none if it was deleted).
pub(super) fn delete_environment(app: &mut App, id: uuid::Uuid) {
    let Some(pos) = app.environments.iter().position(|e| e.id == id) else {
        return;
    };
    let env = app.environments.remove(pos);
    if !app.session_env_ids.remove(&env.id) {
        match CollectionsStore::new(app.config.collections_dir.clone()) {
            Ok(store) => {
                if let Err(e) = store.delete_environment(&env) {
                    tracing::warn!("Failed to delete environment '{}': {}", env.name, e);
                }
            }
            Err(e) => tracing::warn!("Failed to init collections store: {}", e),
        }
    }
    app.active_env = match app.active_env {
        Some(a) if a == pos => None,
        Some(a) if a > pos => Some(a - 1),
        other => other,
    };
    app.editor.env_selected = 0;
    app.remember_active_env();
    app.notify(
        format!("Deleted environment '{}'", env.name),
        NotificationKind::Success,
    );
}
