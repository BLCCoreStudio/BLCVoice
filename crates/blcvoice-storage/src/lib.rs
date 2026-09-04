use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

const SCHEMA_VERSION: i64 = 1;
const MAX_LIST_LIMIT: u32 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvocationSource {
    Shortcut,
    DesktopUi,
}

impl InvocationSource {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Shortcut => "shortcut",
            Self::DesktopUi => "desktop_ui",
        }
    }

    fn parse(value: &str) -> Result<Self, StorageError> {
        match value {
            "shortcut" => Ok(Self::Shortcut),
            "desktop_ui" => Ok(Self::DesktopUi),
            other => Err(StorageError::CorruptData(format!(
                "unknown invocation source `{other}`"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryState {
    TranscribedOnly,
    BackendSubmittedUnverified,
    DeliveredVerified,
    InsertionFailed,
}

impl DeliveryState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::TranscribedOnly => "transcribed_only",
            Self::BackendSubmittedUnverified => "backend_submitted_unverified",
            Self::DeliveredVerified => "delivered_verified",
            Self::InsertionFailed => "insertion_failed",
        }
    }

    fn parse(value: &str) -> Result<Self, StorageError> {
        match value {
            "transcribed_only" => Ok(Self::TranscribedOnly),
            "backend_submitted_unverified" => Ok(Self::BackendSubmittedUnverified),
            "delivered_verified" => Ok(Self::DeliveredVerified),
            "insertion_failed" => Ok(Self::InsertionFailed),
            other => Err(StorageError::CorruptData(format!(
                "unknown delivery state `{other}`"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewHistoryEntry {
    pub created_at_unix_ms: i64,
    pub transcript: String,
    pub invocation_source: InvocationSource,
    pub engine_id: String,
    pub model_id: String,
    pub detected_language: Option<String>,
    pub insertion_backend: Option<String>,
    pub delivery_state: DeliveryState,
}

impl NewHistoryEntry {
    fn validate(&self) -> Result<(), StorageError> {
        if self.transcript.trim().is_empty() {
            return Err(StorageError::InvalidInput(
                "history transcript cannot be empty".to_owned(),
            ));
        }
        if self.engine_id.trim().is_empty() {
            return Err(StorageError::InvalidInput(
                "history engine id cannot be empty".to_owned(),
            ));
        }
        if self.model_id.trim().is_empty() {
            return Err(StorageError::InvalidInput(
                "history model id cannot be empty".to_owned(),
            ));
        }
        if self
            .detected_language
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(StorageError::InvalidInput(
                "detected language cannot be blank".to_owned(),
            ));
        }
        if self
            .insertion_backend
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(StorageError::InvalidInput(
                "insertion backend cannot be blank".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub id: i64,
    pub created_at_unix_ms: i64,
    pub transcript: String,
    pub invocation_source: InvocationSource,
    pub engine_id: String,
    pub model_id: String,
    pub detected_language: Option<String>,
    pub insertion_backend: Option<String>,
    pub delivery_state: DeliveryState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    InvalidInput(String),
    InvalidPath(String),
    UnsupportedSchema { found: i64, supported: i64 },
    Database(String),
    CorruptData(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message)
            | Self::InvalidPath(message)
            | Self::Database(message)
            | Self::CorruptData(message) => formatter.write_str(message),
            Self::UnsupportedSchema { found, supported } => write!(
                formatter,
                "history database schema {found} is newer than supported schema {supported}"
            ),
        }
    }
}

impl Error for StorageError {}

impl From<rusqlite::Error> for StorageError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error.to_string())
    }
}

#[derive(Debug)]
pub struct HistoryStore {
    path: PathBuf,
    connection: Mutex<Connection>,
}

impl HistoryStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref();
        if path.as_os_str().is_empty() {
            return Err(StorageError::InvalidPath(
                "history database path cannot be empty".to_owned(),
            ));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                StorageError::InvalidPath(format!(
                    "could not create history database directory {}: {error}",
                    parent.display()
                ))
            })?;
        }

        let mut connection = Connection::open(path).map_err(|error| {
            StorageError::Database(format!(
                "could not open history database {}: {error}",
                path.display()
            ))
        })?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&mut connection)?;

        Ok(Self {
            path: path.to_path_buf(),
            connection: Mutex::new(connection),
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&self, entry: &NewHistoryEntry) -> Result<HistoryEntry, StorageError> {
        entry.validate()?;
        let mut connection = self.lock_connection();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StorageError::from)?;
        transaction.execute(
            "INSERT INTO history_entries (
                created_at_unix_ms,
                transcript,
                invocation_source,
                engine_id,
                model_id,
                detected_language,
                insertion_backend,
                delivery_state
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                entry.created_at_unix_ms,
                entry.transcript,
                entry.invocation_source.as_str(),
                entry.engine_id,
                entry.model_id,
                entry.detected_language,
                entry.insertion_backend,
                entry.delivery_state.as_str(),
            ],
        )?;
        let id = transaction.last_insert_rowid();
        transaction.commit()?;
        Ok(HistoryEntry {
            id,
            created_at_unix_ms: entry.created_at_unix_ms,
            transcript: entry.transcript.clone(),
            invocation_source: entry.invocation_source,
            engine_id: entry.engine_id.clone(),
            model_id: entry.model_id.clone(),
            detected_language: entry.detected_language.clone(),
            insertion_backend: entry.insertion_backend.clone(),
            delivery_state: entry.delivery_state,
        })
    }

    pub fn list_recent(&self, limit: u32) -> Result<Vec<HistoryEntry>, StorageError> {
        if limit == 0 || limit > MAX_LIST_LIMIT {
            return Err(StorageError::InvalidInput(format!(
                "history list limit must be between 1 and {MAX_LIST_LIMIT}"
            )));
        }
        let connection = self.lock_connection();
        let mut statement = connection.prepare(
            "SELECT
                id,
                created_at_unix_ms,
                transcript,
                invocation_source,
                engine_id,
                model_id,
                detected_language,
                insertion_backend,
                delivery_state
             FROM history_entries
             ORDER BY created_at_unix_ms DESC, id DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map([i64::from(limit)], map_row)?;
        let mut entries = Vec::with_capacity(limit as usize);
        for row in rows {
            entries.push(row??);
        }
        Ok(entries)
    }

    pub fn delete(&self, id: i64) -> Result<bool, StorageError> {
        if id <= 0 {
            return Err(StorageError::InvalidInput(
                "history entry id must be positive".to_owned(),
            ));
        }
        let mut connection = self.lock_connection();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StorageError::from)?;
        let deleted = transaction.execute("DELETE FROM history_entries WHERE id = ?1", [id])?;
        transaction.commit()?;
        Ok(deleted == 1)
    }

    pub fn get(&self, id: i64) -> Result<Option<HistoryEntry>, StorageError> {
        if id <= 0 {
            return Err(StorageError::InvalidInput(
                "history entry id must be positive".to_owned(),
            ));
        }
        let connection = self.lock_connection();
        let mut statement = connection.prepare(
            "SELECT
                id,
                created_at_unix_ms,
                transcript,
                invocation_source,
                engine_id,
                model_id,
                detected_language,
                insertion_backend,
                delivery_state
             FROM history_entries
             WHERE id = ?1",
        )?;
        statement
            .query_row([id], map_row)
            .optional()
            .map_err(StorageError::from)?
            .transpose()
    }

    fn lock_connection(&self) -> MutexGuard<'_, Connection> {
        self.connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn migrate(connection: &mut Connection) -> Result<(), StorageError> {
    let schema_version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if schema_version > SCHEMA_VERSION {
        return Err(StorageError::UnsupportedSchema {
            found: schema_version,
            supported: SCHEMA_VERSION,
        });
    }
    if schema_version == SCHEMA_VERSION {
        ensure_schema_present(connection)?;
        return Ok(());
    }

    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(StorageError::from)?;
    if schema_version == 0 {
        transaction.execute_batch(
            "CREATE TABLE history_entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at_unix_ms INTEGER NOT NULL,
                transcript TEXT NOT NULL CHECK(length(trim(transcript)) > 0),
                invocation_source TEXT NOT NULL CHECK(invocation_source IN ('shortcut', 'desktop_ui')),
                engine_id TEXT NOT NULL CHECK(length(trim(engine_id)) > 0),
                model_id TEXT NOT NULL CHECK(length(trim(model_id)) > 0),
                detected_language TEXT,
                insertion_backend TEXT,
                delivery_state TEXT NOT NULL CHECK(delivery_state IN (
                    'transcribed_only',
                    'backend_submitted_unverified',
                    'delivered_verified',
                    'insertion_failed'
                ))
            );
            CREATE INDEX history_entries_recent_idx
                ON history_entries(created_at_unix_ms DESC, id DESC);
            PRAGMA user_version = 1;",
        )?;
    }
    transaction.commit()?;
    Ok(())
}

fn ensure_schema_present(connection: &Connection) -> Result<(), StorageError> {
    let exists: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'history_entries'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(StorageError::CorruptData(
            "history database declares schema version 1 but history_entries is missing".to_owned(),
        ));
    }
    Ok(())
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<HistoryEntry, StorageError>> {
    let invocation_source: String = row.get(3)?;
    let delivery_state: String = row.get(8)?;
    Ok(Ok(HistoryEntry {
        id: row.get(0)?,
        created_at_unix_ms: row.get(1)?,
        transcript: row.get(2)?,
        invocation_source: InvocationSource::parse(&invocation_source)?,
        engine_id: row.get(4)?,
        model_id: row.get(5)?,
        detected_language: row.get(6)?,
        insertion_backend: row.get(7)?,
        delivery_state: DeliveryState::parse(&delivery_state)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample(created_at_unix_ms: i64, transcript: &str) -> NewHistoryEntry {
        NewHistoryEntry {
            created_at_unix_ms,
            transcript: transcript.to_owned(),
            invocation_source: InvocationSource::Shortcut,
            engine_id: "transcribe.cpp".to_owned(),
            model_id: "base".to_owned(),
            detected_language: Some("tr".to_owned()),
            insertion_backend: Some("test-backend".to_owned()),
            delivery_state: DeliveryState::BackendSubmittedUnverified,
        }
    }

    #[test]
    fn opens_migrates_and_reopens_persisted_history() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("history.sqlite3");
        let store = HistoryStore::open(&path).expect("open");
        let entry = store.append(&sample(10, "merhaba dünya")).expect("append");
        drop(store);

        let reopened = HistoryStore::open(&path).expect("reopen");
        assert_eq!(reopened.get(entry.id).expect("get"), Some(entry));
    }

    #[test]
    fn lists_newest_first_with_stable_id_tiebreak() {
        let directory = tempdir().expect("tempdir");
        let store = HistoryStore::open(directory.path().join("history.sqlite3")).expect("open");
        let first = store.append(&sample(20, "first")).expect("append first");
        let second = store.append(&sample(20, "second")).expect("append second");
        let third = store.append(&sample(30, "third")).expect("append third");

        let listed = store.list_recent(3).expect("list");
        assert_eq!(listed.iter().map(|entry| entry.id).collect::<Vec<_>>(), vec![third.id, second.id, first.id]);
    }

    #[test]
    fn metadata_round_trips_without_upgrading_delivery_truth() {
        let directory = tempdir().expect("tempdir");
        let store = HistoryStore::open(directory.path().join("history.sqlite3")).expect("open");
        let mut input = sample(42, "recoverable transcript");
        input.invocation_source = InvocationSource::DesktopUi;
        input.engine_id = "engine-x".to_owned();
        input.model_id = "model-y".to_owned();
        input.detected_language = None;
        input.insertion_backend = Some("portal-eis".to_owned());
        input.delivery_state = DeliveryState::InsertionFailed;

        let stored = store.append(&input).expect("append");
        assert_eq!(stored.transcript, input.transcript);
        assert_eq!(stored.invocation_source, InvocationSource::DesktopUi);
        assert_eq!(stored.engine_id, "engine-x");
        assert_eq!(stored.model_id, "model-y");
        assert_eq!(stored.detected_language, None);
        assert_eq!(stored.insertion_backend.as_deref(), Some("portal-eis"));
        assert_eq!(stored.delivery_state, DeliveryState::InsertionFailed);
    }

    #[test]
    fn delete_is_explicit_and_idempotent() {
        let directory = tempdir().expect("tempdir");
        let store = HistoryStore::open(directory.path().join("history.sqlite3")).expect("open");
        let entry = store.append(&sample(1, "delete me")).expect("append");
        assert!(store.delete(entry.id).expect("first delete"));
        assert!(!store.delete(entry.id).expect("second delete"));
        assert!(store.get(entry.id).expect("get").is_none());
    }

    #[test]
    fn list_limit_is_bounded() {
        let directory = tempdir().expect("tempdir");
        let store = HistoryStore::open(directory.path().join("history.sqlite3")).expect("open");
        assert!(matches!(store.list_recent(0), Err(StorageError::InvalidInput(_))));
        assert!(matches!(store.list_recent(MAX_LIST_LIMIT + 1), Err(StorageError::InvalidInput(_))));
    }

    #[test]
    fn newer_schema_fails_closed_without_reset() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("history.sqlite3");
        let connection = Connection::open(&path).expect("seed database");
        connection.pragma_update(None, "user_version", SCHEMA_VERSION + 1).expect("set version");
        drop(connection);

        assert!(matches!(
            HistoryStore::open(&path),
            Err(StorageError::UnsupportedSchema { found: 2, supported: 1 })
        ));
        let connection = Connection::open(&path).expect("reopen raw");
        let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0)).expect("read version");
        assert_eq!(version, 2);
    }

    #[test]
    fn corrupt_declared_schema_fails_without_destructive_repair() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("history.sqlite3");
        let connection = Connection::open(&path).expect("seed database");
        connection.pragma_update(None, "user_version", SCHEMA_VERSION).expect("set version");
        drop(connection);

        assert!(matches!(HistoryStore::open(&path), Err(StorageError::CorruptData(_))));
    }
}
