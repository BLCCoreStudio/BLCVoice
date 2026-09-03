use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

const SETTINGS_FILE_NAME: &str = "settings.json";
const SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    schema_version: u32,
    selected_input_device_id: Option<String>,
    selected_model_id: Option<String>,
    language_hint: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            selected_input_device_id: None,
            selected_model_id: None,
            language_hint: None,
        }
    }
}

impl AppSettings {
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    #[must_use]
    pub fn selected_input_device_id(&self) -> Option<&str> {
        self.selected_input_device_id.as_deref()
    }

    #[must_use]
    pub fn selected_model_id(&self) -> Option<&str> {
        self.selected_model_id.as_deref()
    }

    #[must_use]
    pub fn language_hint(&self) -> Option<&str> {
        self.language_hint.as_deref()
    }

    fn normalize(&mut self) {
        self.schema_version = SETTINGS_SCHEMA_VERSION;
        self.selected_input_device_id = normalize_optional(&self.selected_input_device_id);
        self.selected_model_id = normalize_optional(&self.selected_model_id);
        self.language_hint = normalize_language_hint(&self.language_hint);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsError {
    message: String,
}

impl SettingsError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for SettingsError {}

#[derive(Debug)]
pub struct SettingsService {
    path: PathBuf,
    state: Mutex<AppSettings>,
}

impl SettingsService {
    pub fn open(config_dir: impl Into<PathBuf>) -> Result<Self, SettingsError> {
        let config_dir = config_dir.into();
        fs::create_dir_all(&config_dir).map_err(|error| {
            SettingsError::new(format!(
                "could not create BLCVoice config directory {}: {error}",
                config_dir.display()
            ))
        })?;
        let path = config_dir.join(SETTINGS_FILE_NAME);
        let mut settings = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<AppSettings>(&bytes).map_err(|error| {
                SettingsError::new(format!(
                    "could not parse BLCVoice settings {}: {error}",
                    path.display()
                ))
            })?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => AppSettings::default(),
            Err(error) => {
                return Err(SettingsError::new(format!(
                    "could not read BLCVoice settings {}: {error}",
                    path.display()
                )));
            }
        };

        if settings.schema_version > SETTINGS_SCHEMA_VERSION {
            return Err(SettingsError::new(format!(
                "settings schema {} is newer than this BLCVoice build supports ({SETTINGS_SCHEMA_VERSION})",
                settings.schema_version
            )));
        }
        settings.normalize();

        Ok(Self {
            path,
            state: Mutex::new(settings),
        })
    }

    #[must_use]
    pub fn snapshot(&self) -> AppSettings {
        self.lock_state().clone()
    }

    pub fn set_input_device(
        &self,
        device_id: Option<String>,
    ) -> Result<AppSettings, SettingsError> {
        self.update(|settings| settings.selected_input_device_id = device_id)
    }

    pub fn set_model(&self, model_id: Option<String>) -> Result<AppSettings, SettingsError> {
        self.update(|settings| settings.selected_model_id = model_id)
    }

    pub fn set_language_hint(
        &self,
        language_hint: Option<String>,
    ) -> Result<AppSettings, SettingsError> {
        self.update(|settings| settings.language_hint = language_hint)
    }

    fn update(
        &self,
        mutate: impl FnOnce(&mut AppSettings),
    ) -> Result<AppSettings, SettingsError> {
        let mut state = self.lock_state();
        let mut next = state.clone();
        mutate(&mut next);
        next.normalize();
        write_settings(&self.path, &next)?;
        *state = next.clone();
        Ok(next)
    }

    fn lock_state(&self) -> MutexGuard<'_, AppSettings> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn write_settings(path: &Path, settings: &AppSettings) -> Result<(), SettingsError> {
    let parent = path.parent().ok_or_else(|| {
        SettingsError::new(format!("settings path {} has no parent directory", path.display()))
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        SettingsError::new(format!(
            "could not create settings directory {}: {error}",
            parent.display()
        ))
    })?;

    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|error| SettingsError::new(format!("could not encode settings: {error}")))?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| {
            SettingsError::new(format!(
                "could not open temporary settings file {}: {error}",
                temporary.display()
            ))
        })?;
    file.write_all(&bytes).map_err(|error| {
        SettingsError::new(format!(
            "could not write temporary settings file {}: {error}",
            temporary.display()
        ))
    })?;
    file.write_all(b"\n").map_err(|error| {
        SettingsError::new(format!(
            "could not finalize temporary settings file {}: {error}",
            temporary.display()
        ))
    })?;
    file.sync_all().map_err(|error| {
        SettingsError::new(format!(
            "could not sync temporary settings file {}: {error}",
            temporary.display()
        ))
    })?;
    drop(file);

    #[cfg(target_os = "windows")]
    if path.exists() {
        fs::remove_file(path).map_err(|error| {
            SettingsError::new(format!(
                "could not replace existing settings file {}: {error}",
                path.display()
            ))
        })?;
    }

    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        SettingsError::new(format!(
            "could not commit settings file {}: {error}",
            path.display()
        ))
    })
}

fn normalize_optional(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn normalize_language_hint(value: &Option<String>) -> Option<String> {
    let normalized = normalize_optional(value)?;
    if normalized.eq_ignore_ascii_case("auto") {
        None
    } else {
        Some(normalized.to_ascii_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temporary_directory(test_name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock must be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "blcvoice-settings-{test_name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn settings_round_trip_and_normalize_user_values() {
        let directory = temporary_directory("round-trip");
        let service = SettingsService::open(&directory).expect("settings service must open");
        service
            .set_input_device(Some("  mic-1  ".to_owned()))
            .expect("device must save");
        service
            .set_model(Some("  whisper-small-q5km  ".to_owned()))
            .expect("model must save");
        service
            .set_language_hint(Some(" TR ".to_owned()))
            .expect("language must save");

        let reopened = SettingsService::open(&directory).expect("settings must reopen");
        let settings = reopened.snapshot();
        assert_eq!(settings.schema_version(), SETTINGS_SCHEMA_VERSION);
        assert_eq!(settings.selected_input_device_id(), Some("mic-1"));
        assert_eq!(settings.selected_model_id(), Some("whisper-small-q5km"));
        assert_eq!(settings.language_hint(), Some("tr"));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn auto_language_is_stored_as_no_hint() {
        let directory = temporary_directory("auto-language");
        let service = SettingsService::open(&directory).expect("settings service must open");
        let settings = service
            .set_language_hint(Some("AUTO".to_owned()))
            .expect("language must save");
        assert_eq!(settings.language_hint(), None);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn newer_schema_is_rejected_instead_of_overwritten() {
        let directory = temporary_directory("future-schema");
        fs::create_dir_all(&directory).expect("temp directory must exist");
        fs::write(
            directory.join(SETTINGS_FILE_NAME),
            br#"{"schemaVersion":999}"#,
        )
        .expect("future settings must write");

        let error = SettingsService::open(&directory).expect_err("future schema must fail");
        assert!(error.message().contains("newer"));
        let _ = fs::remove_dir_all(directory);
    }
}
