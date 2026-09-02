use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::shortcut::{
    DesktopShortcutService, ShortcutStatusSnapshot, shortcut_decision_name,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutStatusDto {
    backend: &'static str,
    registration: &'static str,
    preferred_trigger: String,
    trigger_description: Option<String>,
    mode: String,
    activations: u64,
    deactivations: u64,
    last_decision: Option<&'static str>,
    last_error: Option<String>,
}

impl From<ShortcutStatusSnapshot> for ShortcutStatusDto {
    fn from(status: ShortcutStatusSnapshot) -> Self {
        Self {
            backend: status.backend.name(),
            registration: status.registration.name(),
            preferred_trigger: status.preferred_trigger,
            trigger_description: status.trigger_description,
            mode: status.mode.to_string(),
            activations: status.activations,
            deactivations: status.deactivations,
            last_decision: status.last_decision.map(shortcut_decision_name),
            last_error: status.last_error,
        }
    }
}

#[tauri::command]
pub fn shortcut_status(
    state: State<'_, Arc<DesktopShortcutService>>,
) -> ShortcutStatusDto {
    ShortcutStatusDto::from(state.status())
}
