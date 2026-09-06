use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use blcvoice_personalization::{
    DictionaryTerm, PersonalizationError, PersonalizationStore, ReplacementRule,
};
use serde::Serialize;
use tauri::State;

use crate::ipc::DesktopState;

const DEFAULT_LIST_LIMIT: u32 = 500;
pub(crate) const DICTIONARY_PROMPT_MAX_BYTES: usize = 1024;

#[derive(Debug)]
struct PersonalizationInner {
    store: Mutex<Option<PersonalizationStore>>,
    database_path: PathBuf,
    last_error: Mutex<Option<String>>,
}

#[derive(Debug, Clone)]
pub struct PersonalizationService {
    inner: Arc<PersonalizationInner>,
}

impl PersonalizationService {
    #[must_use]
    pub fn production(data_dir: PathBuf) -> Self {
        let database_path = data_dir.join("personalization.sqlite3");
        let (store, last_error) = match PersonalizationStore::open(&database_path) {
            Ok(store) => (Some(store), None),
            Err(error) => (None, Some(error.to_string())),
        };
        Self {
            inner: Arc::new(PersonalizationInner {
                store: Mutex::new(store),
                database_path,
                last_error: Mutex::new(last_error),
            }),
        }
    }

    pub(crate) fn dictionary_prompt(&self) -> Result<Option<String>, PersonalizationError> {
        self.with_store(|store| store.dictionary_prompt(DICTIONARY_PROMPT_MAX_BYTES))
    }

    pub(crate) fn apply_replacements(&self, text: &str) -> Result<String, PersonalizationError> {
        self.with_store(|store| store.apply_replacements(text))
    }

    fn list_dictionary(&self, limit: u32) -> Result<Vec<DictionaryTerm>, PersonalizationError> {
        self.with_store(|store| store.list_dictionary_terms(limit))
    }

    fn add_dictionary(&self, term: &str) -> Result<DictionaryTerm, PersonalizationError> {
        self.with_store(|store| store.add_dictionary_term(term, unix_time_ms()))
    }

    fn delete_dictionary(&self, id: i64) -> Result<bool, PersonalizationError> {
        self.with_store(|store| store.delete_dictionary_term(id))
    }

    fn list_replacements(&self, limit: u32) -> Result<Vec<ReplacementRule>, PersonalizationError> {
        self.with_store(|store| store.list_replacements(limit))
    }

    fn add_replacement(
        &self,
        source: &str,
        replacement: &str,
    ) -> Result<ReplacementRule, PersonalizationError> {
        self.with_store(|store| store.add_replacement(source, replacement, unix_time_ms()))
    }

    fn delete_replacement(&self, id: i64) -> Result<bool, PersonalizationError> {
        self.with_store(|store| store.delete_replacement(id))
    }

    fn health(&self) -> PersonalizationHealthDto {
        let store = self.lock_store();
        PersonalizationHealthDto {
            available: store.is_some(),
            database_path: self.inner.database_path.display().to_string(),
            last_error: self.lock_last_error().clone(),
        }
    }

    fn with_store<T>(
        &self,
        operation: impl FnOnce(&PersonalizationStore) -> Result<T, PersonalizationError>,
    ) -> Result<T, PersonalizationError> {
        let store = self.lock_store();
        let Some(store) = store.as_ref() else {
            return Err(PersonalizationError::Database(
                self.lock_last_error()
                    .clone()
                    .unwrap_or_else(|| "personalization database is unavailable".to_owned()),
            ));
        };
        let result = operation(store);
        let mut last_error = self.lock_last_error();
        match &result {
            Ok(_) => *last_error = None,
            Err(error) => *last_error = Some(error.to_string()),
        }
        result
    }

    fn lock_store(&self) -> MutexGuard<'_, Option<PersonalizationStore>> {
        self.inner
            .store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock_last_error(&self) -> MutexGuard<'_, Option<String>> {
        self.inner
            .last_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalizationHealthDto {
    available: bool,
    database_path: String,
    last_error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryTermDto {
    id: i64,
    term: String,
    created_at_unix_ms: i64,
}

impl From<DictionaryTerm> for DictionaryTermDto {
    fn from(term: DictionaryTerm) -> Self {
        Self {
            id: term.id,
            term: term.term,
            created_at_unix_ms: term.created_at_unix_ms,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplacementRuleDto {
    id: i64,
    source: String,
    replacement: String,
    created_at_unix_ms: i64,
}

impl From<ReplacementRule> for ReplacementRuleDto {
    fn from(rule: ReplacementRule) -> Self {
        Self {
            id: rule.id,
            source: rule.source,
            replacement: rule.replacement,
            created_at_unix_ms: rule.created_at_unix_ms,
        }
    }
}

#[tauri::command]
pub fn personalization_status(state: State<'_, DesktopState>) -> PersonalizationHealthDto {
    state.personalization().health()
}

#[tauri::command]
pub fn dictionary_list(
    state: State<'_, DesktopState>,
    limit: Option<u32>,
) -> Result<Vec<DictionaryTermDto>, String> {
    state
        .personalization()
        .list_dictionary(limit.unwrap_or(DEFAULT_LIST_LIMIT))
        .map(|terms| terms.into_iter().map(DictionaryTermDto::from).collect())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn dictionary_add(
    state: State<'_, DesktopState>,
    term: String,
) -> Result<DictionaryTermDto, String> {
    state
        .personalization()
        .add_dictionary(&term)
        .map(DictionaryTermDto::from)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn dictionary_delete(
    state: State<'_, DesktopState>,
    id: i64,
) -> Result<bool, String> {
    state
        .personalization()
        .delete_dictionary(id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn replacement_list(
    state: State<'_, DesktopState>,
    limit: Option<u32>,
) -> Result<Vec<ReplacementRuleDto>, String> {
    state
        .personalization()
        .list_replacements(limit.unwrap_or(DEFAULT_LIST_LIMIT))
        .map(|rules| rules.into_iter().map(ReplacementRuleDto::from).collect())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn replacement_add(
    state: State<'_, DesktopState>,
    source: String,
    replacement: String,
) -> Result<ReplacementRuleDto, String> {
    state
        .personalization()
        .add_replacement(&source, &replacement)
        .map(ReplacementRuleDto::from)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn replacement_delete(
    state: State<'_, DesktopState>,
    id: i64,
) -> Result<bool, String> {
    state
        .personalization()
        .delete_replacement(id)
        .map_err(|error| error.to_string())
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_prompt_budget_is_bounded_for_runtime_use() {
        assert!(DICTIONARY_PROMPT_MAX_BYTES <= 4096);
    }
}
