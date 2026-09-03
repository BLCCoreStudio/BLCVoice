use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const HISTORY_FILE_NAME: &str = "history.json";
const HISTORY_SCHEMA_VERSION: u32 = 1;
pub const MAX_HISTORY_ENTRIES: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    id: u64,
    created_at_unix_ms: u64,
    text: String,
    detected_language: Option<String>,
    engine_id: String,
    model_id: String,
    insertion_backend: String,
    semantic_delivery_verified: bool,
}

impl HistoryEntry {
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }
}

#[derive(Debug, Clone)]
pub struct NewHistoryEntry {
    pub text: String,
    pub detected_language: Option<String>,
    pub engine_id: String,
    pub model_id: String,
    pub insertion_backend: String,
    pub semantic_delivery_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct HistoryDocument {
    schema_version: u32,
    next_id: u64,
    entries: Vec<HistoryEntry>,
}

impl Default for HistoryDocument {
    fn default() -> Self {
        Self {
            schema_version: HISTORY_SCHEMA_VERSION,
            next_id: 1,
            entries: Vec::new(),
        }
    }
}

impl HistoryDocument {
    fn normalize(&mut self) -> Result<(), HistoryError> {
        if self.schema_version > HISTORY_SCHEMA_VERSION {
            return Err(HistoryError::new(format!(
                "history schema {} is newer than this BLCVoice build supports ({HISTORY_SCHEMA_VERSION})",
                self.schema_version
            )));
        }
        self.schema_version = HISTORY_SCHEMA_VERSION;
        self.entries.retain(|entry| !entry.text.is_empty());
        if self.entries.len() > MAX_HISTORY_ENTRIES {
            self.entries.truncate(MAX_HISTORY_ENTRIES);
        }
        let next_after_max = self
            .entries
            .iter()
            .map(|entry| entry.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| HistoryError::new("history entry id space exhausted"))?;
        self.next_id = self.next_id.max(next_after_max).max(1);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryError {
    message: String,
}

impl HistoryError {
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

impl fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for HistoryError {}

#[derive(Debug)]
pub struct HistoryService {
    path: PathBuf,
    state: Mutex<HistoryDocument>,
}

impl HistoryService {
    pub fn open(data_dir: impl Into<PathBuf>) -> Result<Self, HistoryError> {
        let data_dir = data_dir.into();
        fs::create_dir_all(&data_dir).map_err(|error| {
            HistoryError::new(format!(
                "could not create BLCVoice data directory {}: {error}",
                data_dir.display()
            ))
        })?;
        let path = data_dir.join(HISTORY_FILE_NAME);
        let mut document = match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<HistoryDocument>(&bytes) {
                Ok(document) => document,
                Err(error) => {
                    preserve_corrupt_history(&path)?;
                    eprintln!("BLCVoice preserved an unreadable history file: {error}");
                    HistoryDocument::default()
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => HistoryDocument::default(),
            Err(error) => {
                return Err(HistoryError::new(format!(
                    "could not read BLCVoice history {}: {error}",
                    path.display()
                )));
            }
        };
        document.normalize()?;

        Ok(Self {
            path,
            state: Mutex::new(document),
        })
    }

    #[must_use]
    pub fn entries(&self) -> Vec<HistoryEntry> {
        self.lock_state().entries.clone()
    }

    pub fn append(&self, new_entry: NewHistoryEntry) -> Result<HistoryEntry, HistoryError> {
        if new_entry.text.is_empty() {
            return Err(HistoryError::new("history transcript cannot be empty"));
        }
        let mut state = self.lock_state();
        let id = state.next_id;
        state.next_id = id
            .checked_add(1)
            .ok_or_else(|| HistoryError::new("history entry id space exhausted"))?;
        let entry = HistoryEntry {
            id,
            created_at_unix_ms: unix_time_ms()?,
            text: new_entry.text,
            detected_language: new_entry.detected_language,
            engine_id: new_entry.engine_id,
            model_id: new_entry.model_id,
            insertion_backend: new_entry.insertion_backend,
            semantic_delivery_verified: new_entry.semantic_delivery_verified,
        };
        state.entries.insert(0, entry.clone());
        if state.entries.len() > MAX_HISTORY_ENTRIES {
            state.entries.truncate(MAX_HISTORY_ENTRIES);
        }
        write_history(&self.path, &state)?;
        Ok(entry)
    }

    pub fn delete(&self, id: u64) -> Result<bool, HistoryError> {
        let mut state = self.lock_state();
        let before = state.entries.len();
        state.entries.retain(|entry| entry.id != id);
        let changed = state.entries.len() != before;
        if changed {
            write_history(&self.path, &state)?;
        }
        Ok(changed)
    }

    pub fn clear(&self) -> Result<(), HistoryError> {
        let mut state = self.lock_state();
        if state.entries.is_empty() {
            return Ok(());
        }
        state.entries.clear();
        write_history(&self.path, &state)
    }

    fn lock_state(&self) -> MutexGuard<'_, HistoryDocument> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn unix_time_ms() -> Result<u64, HistoryError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| HistoryError::new(format!("system clock is before Unix epoch: {error}")))?
        .as_millis();
    u64::try_from(millis).map_err(|_| HistoryError::new("system timestamp is too large"))
}

fn preserve_corrupt_history(path: &Path) -> Result<(), HistoryError> {
    let stamp = unix_time_ms()?;
    let backup = path.with_extension(format!("json.corrupt-{stamp}"));
    fs::rename(path, &backup).map_err(|error| {
        HistoryError::new(format!(
            "could not preserve unreadable history {} as {}: {error}",
            path.display(),
            backup.display()
        ))
    })
}

fn write_history(path: &Path, document: &HistoryDocument) -> Result<(), HistoryError> {
    let parent = path.parent().ok_or_else(|| {
        HistoryError::new(format!("history path {} has no parent directory", path.display()))
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        HistoryError::new(format!(
            "could not create history directory {}: {error}",
            parent.display()
        ))
    })?;

    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(document)
        .map_err(|error| HistoryError::new(format!("could not encode history: {error}")))?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| {
            HistoryError::new(format!(
                "could not open temporary history file {}: {error}",
                temporary.display()
            ))
        })?;
    file.write_all(&bytes).map_err(|error| {
        HistoryError::new(format!(
            "could not write temporary history file {}: {error}",
            temporary.display()
        ))
    })?;
    file.write_all(b"\n").map_err(|error| {
        HistoryError::new(format!(
            "could not finalize temporary history file {}: {error}",
            temporary.display()
        ))
    })?;
    file.sync_all().map_err(|error| {
        HistoryError::new(format!(
            "could not sync temporary history file {}: {error}",
            temporary.display()
        ))
    })?;
    drop(file);

    #[cfg(target_os = "windows")]
    if path.exists() {
        fs::remove_file(path).map_err(|error| {
            HistoryError::new(format!(
                "could not replace existing history file {}: {error}",
                path.display()
            ))
        })?;
    }

    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        HistoryError::new(format!(
            "could not commit history file {}: {error}",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_directory(test_name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock must be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "blcvoice-history-{test_name}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn entry(text: impl Into<String>) -> NewHistoryEntry {
        NewHistoryEntry {
            text: text.into(),
            detected_language: Some("tr".to_owned()),
            engine_id: "transcribe.cpp".to_owned(),
            model_id: "test-model".to_owned(),
            insertion_backend: "test".to_owned(),
            semantic_delivery_verified: false,
        }
    }

    #[test]
    fn history_round_trips_and_newest_entry_is_first() {
        let directory = temporary_directory("round-trip");
        let service = HistoryService::open(&directory).unwrap();
        let first = service.append(entry("first")).unwrap();
        let second = service.append(entry("second")).unwrap();
        assert!(second.id() > first.id());
        drop(service);

        let reopened = HistoryService::open(&directory).unwrap();
        let entries = reopened.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "second");
        assert_eq!(entries[1].text, "first");
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn history_is_bounded_to_the_latest_entries() {
        let directory = temporary_directory("bounded");
        let service = HistoryService::open(&directory).unwrap();
        for index in 0..(MAX_HISTORY_ENTRIES + 5) {
            service.append(entry(format!("entry {index}"))).unwrap();
        }
        let entries = service.entries();
        assert_eq!(entries.len(), MAX_HISTORY_ENTRIES);
        assert_eq!(entries[0].text, format!("entry {}", MAX_HISTORY_ENTRIES + 4));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn delete_and_clear_are_persistent() {
        let directory = temporary_directory("delete-clear");
        let service = HistoryService::open(&directory).unwrap();
        let item = service.append(entry("keep briefly")).unwrap();
        assert!(service.delete(item.id()).unwrap());
        service.append(entry("clear me")).unwrap();
        service.clear().unwrap();
        drop(service);
        assert!(HistoryService::open(&directory).unwrap().entries().is_empty());
        let _ = fs::remove_dir_all(directory);
    }
}
