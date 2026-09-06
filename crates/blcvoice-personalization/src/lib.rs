#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

const SCHEMA_VERSION: i64 = 1;
const MAX_TERM_BYTES: usize = 256;
const MAX_REPLACEMENT_BYTES: usize = 1024;
const MAX_LIST_LIMIT: u32 = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryTerm {
    pub id: i64,
    pub term: String,
    pub created_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementRule {
    pub id: i64,
    pub source: String,
    pub replacement: String,
    pub created_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersonalizationError {
    InvalidInput(String),
    InvalidPath(String),
    UnsupportedSchema { found: i64, supported: i64 },
    Database(String),
    CorruptData(String),
}

impl fmt::Display for PersonalizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message)
            | Self::InvalidPath(message)
            | Self::Database(message)
            | Self::CorruptData(message) => formatter.write_str(message),
            Self::UnsupportedSchema { found, supported } => write!(
                formatter,
                "personalization database schema {found} is newer than supported schema {supported}"
            ),
        }
    }
}

impl Error for PersonalizationError {}

impl From<rusqlite::Error> for PersonalizationError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error.to_string())
    }
}

#[derive(Debug)]
pub struct PersonalizationStore {
    path: PathBuf,
    connection: Mutex<Connection>,
}

impl PersonalizationStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PersonalizationError> {
        let path = path.as_ref();
        if path.as_os_str().is_empty() {
            return Err(PersonalizationError::InvalidPath(
                "personalization database path cannot be empty".to_owned(),
            ));
        }
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(|error| {
                PersonalizationError::InvalidPath(format!(
                    "could not create personalization database directory {}: {error}",
                    parent.display()
                ))
            })?;
        }

        let mut connection = Connection::open(path).map_err(|error| {
            PersonalizationError::Database(format!(
                "could not open personalization database {}: {error}",
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

    pub fn add_dictionary_term(
        &self,
        term: &str,
        created_at_unix_ms: i64,
    ) -> Result<DictionaryTerm, PersonalizationError> {
        let term = validate_non_empty("dictionary term", term, MAX_TERM_BYTES)?;
        let mut connection = self.lock_connection();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(PersonalizationError::from)?;
        transaction.execute(
            "INSERT INTO dictionary_terms(term, created_at_unix_ms)
             VALUES (?1, ?2)
             ON CONFLICT(term) DO UPDATE SET term = excluded.term",
            params![term, created_at_unix_ms],
        )?;
        let id: i64 = transaction.query_row(
            "SELECT id FROM dictionary_terms WHERE term = ?1",
            [term],
            |row| row.get(0),
        )?;
        transaction.commit()?;
        Ok(DictionaryTerm {
            id,
            term: term.to_owned(),
            created_at_unix_ms,
        })
    }

    pub fn list_dictionary_terms(
        &self,
        limit: u32,
    ) -> Result<Vec<DictionaryTerm>, PersonalizationError> {
        validate_limit(limit)?;
        let connection = self.lock_connection();
        let mut statement = connection.prepare(
            "SELECT id, term, created_at_unix_ms
             FROM dictionary_terms
             ORDER BY lower(term), id
             LIMIT ?1",
        )?;
        let rows = statement.query_map([i64::from(limit)], |row| {
            Ok(DictionaryTerm {
                id: row.get(0)?,
                term: row.get(1)?,
                created_at_unix_ms: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(PersonalizationError::from)
    }

    pub fn delete_dictionary_term(&self, id: i64) -> Result<bool, PersonalizationError> {
        delete_by_id(&self.connection, "dictionary_terms", id)
    }

    pub fn dictionary_prompt(&self, max_bytes: usize) -> Result<Option<String>, PersonalizationError> {
        if max_bytes == 0 {
            return Ok(None);
        }
        let terms = self.list_dictionary_terms(MAX_LIST_LIMIT)?;
        let mut prompt = String::new();
        for term in terms {
            let separator = if prompt.is_empty() { "" } else { ", " };
            if prompt.len() + separator.len() + term.term.len() > max_bytes {
                break;
            }
            prompt.push_str(separator);
            prompt.push_str(&term.term);
        }
        Ok((!prompt.is_empty()).then_some(prompt))
    }

    pub fn add_replacement(
        &self,
        source: &str,
        replacement: &str,
        created_at_unix_ms: i64,
    ) -> Result<ReplacementRule, PersonalizationError> {
        let source = validate_non_empty("replacement source", source, MAX_TERM_BYTES)?;
        let replacement = validate_non_empty(
            "replacement destination",
            replacement,
            MAX_REPLACEMENT_BYTES,
        )?;
        if source == replacement {
            return Err(PersonalizationError::InvalidInput(
                "replacement source and destination must differ".to_owned(),
            ));
        }

        let mut connection = self.lock_connection();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(PersonalizationError::from)?;
        transaction.execute(
            "INSERT INTO replacement_rules(source, replacement, created_at_unix_ms)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(source) DO UPDATE SET
               replacement = excluded.replacement,
               created_at_unix_ms = excluded.created_at_unix_ms",
            params![source, replacement, created_at_unix_ms],
        )?;
        let (id, stored_created_at): (i64, i64) = transaction.query_row(
            "SELECT id, created_at_unix_ms FROM replacement_rules WHERE source = ?1",
            [source],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        transaction.commit()?;
        Ok(ReplacementRule {
            id,
            source: source.to_owned(),
            replacement: replacement.to_owned(),
            created_at_unix_ms: stored_created_at,
        })
    }

    pub fn list_replacements(
        &self,
        limit: u32,
    ) -> Result<Vec<ReplacementRule>, PersonalizationError> {
        validate_limit(limit)?;
        let connection = self.lock_connection();
        let mut statement = connection.prepare(
            "SELECT id, source, replacement, created_at_unix_ms
             FROM replacement_rules
             ORDER BY length(source) DESC, id ASC
             LIMIT ?1",
        )?;
        let rows = statement.query_map([i64::from(limit)], |row| {
            Ok(ReplacementRule {
                id: row.get(0)?,
                source: row.get(1)?,
                replacement: row.get(2)?,
                created_at_unix_ms: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(PersonalizationError::from)
    }

    pub fn delete_replacement(&self, id: i64) -> Result<bool, PersonalizationError> {
        delete_by_id(&self.connection, "replacement_rules", id)
    }

    pub fn apply_replacements(&self, text: &str) -> Result<String, PersonalizationError> {
        let rules = self.list_replacements(MAX_LIST_LIMIT)?;
        Ok(apply_rules_non_cascading(text, &rules))
    }

    fn lock_connection(&self) -> MutexGuard<'_, Connection> {
        self.connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn validate_non_empty<'a>(
    label: &str,
    value: &'a str,
    max_bytes: usize,
) -> Result<&'a str, PersonalizationError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(PersonalizationError::InvalidInput(format!(
            "{label} cannot be empty"
        )));
    }
    if value.len() > max_bytes {
        return Err(PersonalizationError::InvalidInput(format!(
            "{label} cannot exceed {max_bytes} UTF-8 bytes"
        )));
    }
    if value.contains('\0') {
        return Err(PersonalizationError::InvalidInput(format!(
            "{label} cannot contain NUL characters"
        )));
    }
    Ok(value)
}

fn validate_limit(limit: u32) -> Result<(), PersonalizationError> {
    if limit == 0 || limit > MAX_LIST_LIMIT {
        return Err(PersonalizationError::InvalidInput(format!(
            "list limit must be between 1 and {MAX_LIST_LIMIT}"
        )));
    }
    Ok(())
}

fn delete_by_id(
    connection: &Mutex<Connection>,
    table: &str,
    id: i64,
) -> Result<bool, PersonalizationError> {
    if id <= 0 {
        return Err(PersonalizationError::InvalidInput(
            "entry id must be positive".to_owned(),
        ));
    }
    let mut connection = connection
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(PersonalizationError::from)?;
    let sql = match table {
        "dictionary_terms" => "DELETE FROM dictionary_terms WHERE id = ?1",
        "replacement_rules" => "DELETE FROM replacement_rules WHERE id = ?1",
        _ => {
            return Err(PersonalizationError::CorruptData(
                "unsupported personalization table".to_owned(),
            ));
        }
    };
    let deleted = transaction.execute(sql, [id])?;
    transaction.commit()?;
    Ok(deleted == 1)
}

fn apply_rules_non_cascading(text: &str, rules: &[ReplacementRule]) -> String {
    if text.is_empty() || rules.is_empty() {
        return text.to_owned();
    }

    let mut ordered = rules.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        right
            .source
            .len()
            .cmp(&left.source.len())
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut output = String::with_capacity(text.len());
    let mut offset = 0;
    while offset < text.len() {
        let remaining = &text[offset..];
        if let Some(rule) = ordered
            .iter()
            .find(|rule| remaining.starts_with(rule.source.as_str()))
        {
            output.push_str(&rule.replacement);
            offset += rule.source.len();
            continue;
        }
        let next = remaining
            .chars()
            .next()
            .expect("non-empty UTF-8 suffix must start with a char");
        output.push(next);
        offset += next.len_utf8();
    }
    output
}

fn migrate(connection: &mut Connection) -> Result<(), PersonalizationError> {
    let schema_version: i64 =
        connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if schema_version > SCHEMA_VERSION {
        return Err(PersonalizationError::UnsupportedSchema {
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
        .map_err(PersonalizationError::from)?;
    if schema_version == 0 {
        transaction.execute_batch(
            "CREATE TABLE dictionary_terms (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                term TEXT NOT NULL UNIQUE CHECK(length(trim(term)) > 0),
                created_at_unix_ms INTEGER NOT NULL
            );
            CREATE TABLE replacement_rules (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source TEXT NOT NULL UNIQUE CHECK(length(trim(source)) > 0),
                replacement TEXT NOT NULL CHECK(length(trim(replacement)) > 0),
                created_at_unix_ms INTEGER NOT NULL,
                CHECK(source <> replacement)
            );
            CREATE INDEX dictionary_terms_recent_idx
                ON dictionary_terms(created_at_unix_ms DESC, id DESC);
            CREATE INDEX replacement_rules_recent_idx
                ON replacement_rules(created_at_unix_ms DESC, id DESC);
            PRAGMA user_version = 1;",
        )?;
    }
    transaction.commit()?;
    Ok(())
}

fn ensure_schema_present(connection: &Connection) -> Result<(), PersonalizationError> {
    for table in ["dictionary_terms", "replacement_rules"] {
        let exists: Option<i64> = connection
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_none() {
            return Err(PersonalizationError::CorruptData(format!(
                "personalization database declares schema version 1 but {table} is missing"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn store() -> PersonalizationStore {
        let directory = tempdir().expect("tempdir");
        let path = directory.keep().join("personalization.sqlite3");
        PersonalizationStore::open(path).expect("open")
    }

    #[test]
    fn dictionary_persists_and_builds_bounded_prompt() {
        let store = store();
        store
            .add_dictionary_term("BLCVoice", 1)
            .expect("add first term");
        store
            .add_dictionary_term("Yapay Zeka", 2)
            .expect("add second term");
        assert_eq!(store.list_dictionary_terms(10).expect("list").len(), 2);
        assert_eq!(
            store.dictionary_prompt(64).expect("prompt"),
            Some("BLCVoice, Yapay Zeka".to_owned())
        );
    }

    #[test]
    fn adding_existing_dictionary_term_is_idempotent() {
        let store = store();
        let first = store.add_dictionary_term("BLCVoice", 1).expect("first");
        let second = store.add_dictionary_term("BLCVoice", 2).expect("second");
        assert_eq!(first.id, second.id);
        assert_eq!(store.list_dictionary_terms(10).expect("list").len(), 1);
    }

    #[test]
    fn replacements_are_non_cascading_and_longest_match_wins() {
        let store = store();
        store
            .add_replacement("AI", "yapay zeka", 1)
            .expect("AI replacement");
        store
            .add_replacement("AI lab", "BLCVoice", 2)
            .expect("long replacement");
        store
            .add_replacement("yapay zeka", "SHOULD NOT CASCADE", 3)
            .expect("cascade trap");

        assert_eq!(
            store.apply_replacements("AI lab ve AI").expect("apply"),
            "BLCVoice ve yapay zeka"
        );
    }

    #[test]
    fn replacement_upsert_keeps_one_rule_per_source() {
        let store = store();
        let first = store.add_replacement("abc", "one", 1).expect("first");
        let second = store.add_replacement("abc", "two", 2).expect("second");
        assert_eq!(first.id, second.id);
        assert_eq!(store.apply_replacements("abc").expect("apply"), "two");
    }

    #[test]
    fn validation_rejects_blank_and_identity_rules() {
        let store = store();
        assert!(matches!(
            store.add_dictionary_term("   ", 1),
            Err(PersonalizationError::InvalidInput(_))
        ));
        assert!(matches!(
            store.add_replacement("same", "same", 1),
            Err(PersonalizationError::InvalidInput(_))
        ));
    }
}
