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
    pub fn open(data_dir: PathBuf) -> Result<Self, HistoryError> {
        let path = data_dir.join(HISTORY_FILE_NAME);
        let mut document = load_document(&path)?;
        document.normalize()?;
        Ok(Self {
            path,
            state: Mutex::new(document),
        })
    }

    #[cfg(test)]
    fn open_at(path: PathBuf) -> Result<Self, HistoryError> {
        let mut document = load_document(&path)?;
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

    pub fn append(&self, entry: NewHistoryEntry) -> Result<HistoryEntry, HistoryError> {
        let mut state = self.lock_state();
        let id = state.next_id;
        state.next_id = state
            .next_id
            .checked_add(1)
            .ok_or_else(|| HistoryError::new("history entry id space exhausted"))?;
        let entry = HistoryEntry {
            id,
            created_at_unix_ms: unix_time_ms()?,
            text: entry.text,
            detected_language: entry.detected_language,
            engine_id: entry.engine_id,
            model_id: entry.model_id,
            insertion_backend: entry.insertion_backend,
            semantic_delivery_verified: entry.semantic_delivery_verified,
        };
        if entry.text.is_empty() {
            return Err(HistoryError::new("history text cannot be empty"));
        }
        state.entries.insert(0, entry.clone());
        if state.entries.len() > MAX_HISTORY_ENTRIES {
            state.entries.truncate(MAX_HISTORY_ENTRIES);
        }
        persist_document(&self.path, &state)?;
        Ok(entry)
    }

    pub fn delete(&self, id: u64) -> Result<bool, HistoryError> {
        let mut state = self.lock_state();
        let original_len = state.entries.len();
        state.entries.retain(|entry| entry.id != id);
        let removed = state.entries.len() != original_len;
        if removed {
            persist_document(&self.path, &state)?;
        }
        Ok(removed)
    }

    pub fn clear(&self) -> Result<(), HistoryError> {
        let mut state = self.lock_state();
        if state.entries.is_empty() {
            return Ok(());
        }
        state.entries.clear();
        persist_document(&self.path, &state)
    }

    fn lock_state(&self) -> MutexGuard<'_, HistoryDocument> {
        match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

fn load_document(path: &Path) -> Result<HistoryDocument, HistoryError> {
    if !path.exists() {
        return Ok(HistoryDocument::default());
    }
    let bytes = fs::read(path)
        .map_err(|error| HistoryError::new(format!("could not read history: {error}")))?;
    if bytes.is_empty() {
        return Ok(HistoryDocument::default());
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| HistoryError::new(format!("could not parse history: {error}")))
}

fn persist_document(path: &Path, document: &HistoryDocument) -> Result<(), HistoryError> {
    let parent = path
        .parent()
        .ok_or_else(|| HistoryError::new("history path has no parent directory"))?;
    fs::create_dir_all(parent)
        .map_err(|error| HistoryError::new(format!("could not create history directory: {error}")))?;
    let bytes = serde_json::to_vec_pretty(document)
        .map_err(|error| HistoryError::new(format!("could not serialize history: {error}")))?;
    let temp = path.with_extension("json.tmp");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temp)
        .map_err(|error| HistoryError::new(format!("could not open history temp file: {error}")))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| HistoryError::new(format!("could not write history: {error}")))?;
    drop(file);
    replace_file(&temp, path)
        .map_err(|error| HistoryError::new(format!("could not commit history: {error}")))
}

fn replace_file(temp: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(target_os = "windows")]
    if destination.exists() {
        fs::remove_file(destination)?;
    }
    fs::rename(temp, destination)
}

fn unix_time_ms() -> Result<u64, HistoryError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| HistoryError::new(format!("system clock predates Unix epoch: {error}")))?;
    u64::try_from(duration.as_millis())
        .map_err(|_| HistoryError::new("system timestamp does not fit in u64 milliseconds"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn temporary_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "blcvoice-history-{label}-{}-{}.json",
            std::process::id(),
            unix_time_ms().expect("time must be available")
        ))
    }

    fn entry(text: &str) -> NewHistoryEntry {
        NewHistoryEntry {
            text: text.to_owned(),
            detected_language: Some("tr".to_owned()),
            engine_id: "transcribe.cpp".to_owned(),
            model_id: "test-model".to_owned(),
            insertion_backend: "test-backend".to_owned(),
            semantic_delivery_verified: false,
        }
    }

    #[test]
    fn history_round_trips_and_newest_entry_is_first() {
        let path = temporary_path("round-trip");
        let service = HistoryService::open_at(path.clone()).expect("history must open");
        let first = service.append(entry("ilk")).expect("first entry");
        std::thread::sleep(Duration::from_millis(2));
        let second = service.append(entry("ikinci")).expect("second entry");
        let entries = service.entries();
        assert_eq!(entries, vec![second.clone(), first.clone()]);
        drop(service);

        let reopened = HistoryService::open_at(path.clone()).expect("history must reopen");
        assert_eq!(reopened.entries(), vec![second, first]);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn delete_and_clear_are_persistent() {
        let path = temporary_path("delete-clear");
        let service = HistoryService::open_at(path.clone()).expect("history must open");
        let first = service.append(entry("bir")).expect("first entry");
        let _second = service.append(entry("iki")).expect("second entry");
        assert!(service.delete(first.id).expect("delete must succeed"));
        assert_eq!(service.entries().len(), 1);
        service.clear().expect("clear must persist");
        assert!(service.entries().is_empty());
        drop(service);

        let reopened = HistoryService::open_at(path.clone()).expect("history must reopen");
        assert!(reopened.entries().is_empty());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn history_is_bounded_to_the_latest_entries() {
        let path = temporary_path("bounded");
        let service = HistoryService::open_at(path.clone()).expect("history must open");
        for index in 0..(MAX_HISTORY_ENTRIES + 5) {
            service
                .append(entry(&format!("entry-{index}")))
                .expect("append must succeed");
        }
        let entries = service.entries();
        assert_eq!(entries.len(), MAX_HISTORY_ENTRIES);
        assert_eq!(entries[0].text, format!("entry-{}", MAX_HISTORY_ENTRIES + 4));
        assert_eq!(entries.last().expect("last entry").text, "entry-5");
        let _ = fs::remove_file(path);
    }
}
