use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use serde::{Deserialize, Serialize};

const TEXT_RULES_FILE_NAME: &str = "text-rules.json";
const TEXT_RULES_SCHEMA_VERSION: u32 = 1;

static TEXT_RULES: OnceLock<Arc<TextRulesService>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryEntry {
    id: u64,
    term: String,
    aliases: Vec<String>,
    enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReplacementRule {
    id: u64,
    from: String,
    to: String,
    enabled: bool,
    whole_word: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
struct TextRulesDocument {
    schema_version: u32,
    next_id: u64,
    dictionary: Vec<DictionaryEntry>,
    replacements: Vec<ReplacementRule>,
}

impl Default for TextRulesDocument {
    fn default() -> Self {
        Self {
            schema_version: TEXT_RULES_SCHEMA_VERSION,
            next_id: 1,
            dictionary: Vec::new(),
            replacements: Vec::new(),
        }
    }
}

impl TextRulesDocument {
    fn normalize(&mut self) -> Result<(), TextRulesError> {
        self.schema_version = TEXT_RULES_SCHEMA_VERSION;
        let mut highest_id = 0_u64;

        for entry in &mut self.dictionary {
            entry.term = normalize_required(&entry.term, "dictionary term")?;
            entry.aliases = normalize_list(&entry.aliases);
            entry.aliases.retain(|alias| alias != &entry.term);
            highest_id = highest_id.max(entry.id);
        }
        for rule in &mut self.replacements {
            rule.from = normalize_required(&rule.from, "replacement source")?;
            rule.to = normalize_required(&rule.to, "replacement destination")?;
            highest_id = highest_id.max(rule.id);
        }

        self.next_id = self.next_id.max(highest_id.saturating_add(1)).max(1);
        Ok(())
    }

    fn allocate_id(&mut self) -> u64 {
        let id = self.next_id.max(1);
        self.next_id = id.saturating_add(1);
        id
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextRulesSnapshot {
    dictionary: Vec<DictionaryEntry>,
    replacements: Vec<ReplacementRule>,
}

impl From<&TextRulesDocument> for TextRulesSnapshot {
    fn from(document: &TextRulesDocument) -> Self {
        Self {
            dictionary: document.dictionary.clone(),
            replacements: document.replacements.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRulesError {
    message: String,
}

impl TextRulesError {
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

impl fmt::Display for TextRulesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for TextRulesError {}

#[derive(Debug)]
pub struct TextRulesService {
    path: PathBuf,
    state: Mutex<TextRulesDocument>,
}

impl TextRulesService {
    pub fn open(config_dir: impl Into<PathBuf>) -> Result<Self, TextRulesError> {
        let config_dir = config_dir.into();
        fs::create_dir_all(&config_dir).map_err(|error| {
            TextRulesError::new(format!(
                "could not create BLCVoice config directory {}: {error}",
                config_dir.display()
            ))
        })?;
        let path = config_dir.join(TEXT_RULES_FILE_NAME);
        let mut document = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<TextRulesDocument>(&bytes).map_err(|error| {
                TextRulesError::new(format!(
                    "could not parse BLCVoice text rules {}: {error}",
                    path.display()
                ))
            })?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => TextRulesDocument::default(),
            Err(error) => {
                return Err(TextRulesError::new(format!(
                    "could not read BLCVoice text rules {}: {error}",
                    path.display()
                )));
            }
        };

        if document.schema_version > TEXT_RULES_SCHEMA_VERSION {
            return Err(TextRulesError::new(format!(
                "text-rules schema {} is newer than this BLCVoice build supports ({TEXT_RULES_SCHEMA_VERSION})",
                document.schema_version
            )));
        }
        document.normalize()?;

        Ok(Self {
            path,
            state: Mutex::new(document),
        })
    }

    #[must_use]
    pub fn snapshot(&self) -> TextRulesSnapshot {
        TextRulesSnapshot::from(&*self.lock_state())
    }

    pub fn save_dictionary(
        &self,
        id: Option<u64>,
        term: String,
        aliases: Vec<String>,
        enabled: Option<bool>,
    ) -> Result<DictionaryEntry, TextRulesError> {
        let term = normalize_required(&term, "dictionary term")?;
        let mut aliases = normalize_list(&aliases);
        aliases.retain(|alias| alias != &term);
        self.update(|document| {
            if let Some(id) = id {
                let entry = document
                    .dictionary
                    .iter_mut()
                    .find(|entry| entry.id == id)
                    .ok_or_else(|| TextRulesError::new(format!("dictionary entry {id} does not exist")))?;
                entry.term = term;
                entry.aliases = aliases;
                entry.enabled = enabled.unwrap_or(entry.enabled);
                return Ok(entry.clone());
            }

            let entry = DictionaryEntry {
                id: document.allocate_id(),
                term,
                aliases,
                enabled: enabled.unwrap_or(true),
            };
            document.dictionary.push(entry.clone());
            Ok(entry)
        })
    }

    pub fn delete_dictionary(&self, id: u64) -> Result<bool, TextRulesError> {
        self.update(|document| {
            let before = document.dictionary.len();
            document.dictionary.retain(|entry| entry.id != id);
            Ok(document.dictionary.len() != before)
        })
    }

    pub fn save_replacement(
        &self,
        id: Option<u64>,
        from: String,
        to: String,
        enabled: Option<bool>,
        whole_word: Option<bool>,
    ) -> Result<ReplacementRule, TextRulesError> {
        let from = normalize_required(&from, "replacement source")?;
        let to = normalize_required(&to, "replacement destination")?;
        self.update(|document| {
            if let Some(id) = id {
                let rule = document
                    .replacements
                    .iter_mut()
                    .find(|rule| rule.id == id)
                    .ok_or_else(|| TextRulesError::new(format!("replacement rule {id} does not exist")))?;
                rule.from = from;
                rule.to = to;
                rule.enabled = enabled.unwrap_or(rule.enabled);
                rule.whole_word = whole_word.unwrap_or(rule.whole_word);
                return Ok(rule.clone());
            }

            let rule = ReplacementRule {
                id: document.allocate_id(),
                from,
                to,
                enabled: enabled.unwrap_or(true),
                whole_word: whole_word.unwrap_or(true),
            };
            document.replacements.push(rule.clone());
            Ok(rule)
        })
    }

    pub fn delete_replacement(&self, id: u64) -> Result<bool, TextRulesError> {
        self.update(|document| {
            let before = document.replacements.len();
            document.replacements.retain(|rule| rule.id != id);
            Ok(document.replacements.len() != before)
        })
    }

    #[must_use]
    pub fn apply(&self, text: &str) -> String {
        let state = self.lock_state();
        apply_document(text, &state)
    }

    fn update<T>(
        &self,
        mutate: impl FnOnce(&mut TextRulesDocument) -> Result<T, TextRulesError>,
    ) -> Result<T, TextRulesError> {
        let mut state = self.lock_state();
        let mut next = state.clone();
        let result = mutate(&mut next)?;
        next.normalize()?;
        write_document(&self.path, &next)?;
        *state = next;
        Ok(result)
    }

    fn lock_state(&self) -> MutexGuard<'_, TextRulesDocument> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub fn initialize(config_dir: PathBuf) -> Result<(), TextRulesError> {
    if TEXT_RULES.get().is_some() {
        return Ok(());
    }
    let service = Arc::new(TextRulesService::open(config_dir)?);
    TEXT_RULES
        .set(service)
        .map_err(|_| TextRulesError::new("text rules service was initialized twice"))
}

fn global() -> Result<&'static Arc<TextRulesService>, TextRulesError> {
    TEXT_RULES
        .get()
        .ok_or_else(|| TextRulesError::new("text rules service is not initialized"))
}

#[must_use]
pub fn apply_text_rules(text: &str) -> String {
    global()
        .map(|service| service.apply(text))
        .unwrap_or_else(|_| text.to_owned())
}

#[tauri::command]
pub fn text_rules_snapshot() -> Result<TextRulesSnapshot, String> {
    global()
        .map(|service| service.snapshot())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn dictionary_save(
    id: Option<u64>,
    term: String,
    aliases: Vec<String>,
    enabled: Option<bool>,
) -> Result<DictionaryEntry, String> {
    global()?
        .save_dictionary(id, term, aliases, enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn dictionary_delete(id: u64) -> Result<bool, String> {
    global()?
        .delete_dictionary(id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn replacement_save(
    id: Option<u64>,
    from: String,
    to: String,
    enabled: Option<bool>,
    whole_word: Option<bool>,
) -> Result<ReplacementRule, String> {
    global()?
        .save_replacement(id, from, to, enabled, whole_word)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn replacement_delete(id: u64) -> Result<bool, String> {
    global()?
        .delete_replacement(id)
        .map_err(|error| error.to_string())
}

fn apply_document(text: &str, document: &TextRulesDocument) -> String {
    let mut output = text.to_owned();

    for entry in document.dictionary.iter().filter(|entry| entry.enabled) {
        let mut sources = entry.aliases.clone();
        sources.push(entry.term.clone());
        sources.sort_by_key(|source| std::cmp::Reverse(source.chars().count()));
        sources.dedup();
        for source in sources {
            output = replace_literal(&output, &source, &entry.term, true, false);
        }
    }

    for rule in document.replacements.iter().filter(|rule| rule.enabled) {
        output = replace_literal(&output, &rule.from, &rule.to, rule.whole_word, false);
    }
    output
}

fn replace_literal(
    input: &str,
    needle: &str,
    replacement: &str,
    whole_word: bool,
    case_sensitive: bool,
) -> String {
    if needle.is_empty() || input.is_empty() {
        return input.to_owned();
    }

    let folded_input;
    let folded_needle;
    let (haystack, search) = if case_sensitive {
        (input, needle)
    } else {
        folded_input = input.to_lowercase();
        folded_needle = needle.to_lowercase();
        if folded_input.len() == input.len() && folded_needle.len() == needle.len() {
            (folded_input.as_str(), folded_needle.as_str())
        } else {
            // Unicode lowercase can change byte length for a small set of characters.
            // Falling back to exact matching keeps byte offsets correct and UTF-8 safe.
            (input, needle)
        }
    };

    let mut output = String::with_capacity(input.len());
    let mut last = 0;
    let mut changed = false;
    for (start, _) in haystack.match_indices(search) {
        let end = start + search.len();
        if start < last || (whole_word && !has_word_boundaries(input, start, end)) {
            continue;
        }
        output.push_str(&input[last..start]);
        output.push_str(replacement);
        last = end;
        changed = true;
    }
    if !changed {
        return input.to_owned();
    }
    output.push_str(&input[last..]);
    output
}

fn has_word_boundaries(input: &str, start: usize, end: usize) -> bool {
    let previous = input[..start].chars().next_back();
    let next = input[end..].chars().next();
    !previous.is_some_and(is_word_character) && !next.is_some_and(is_word_character)
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn normalize_required(value: &str, label: &str) -> Result<String, TextRulesError> {
    let value = value.trim();
    if value.is_empty() {
        Err(TextRulesError::new(format!("{label} cannot be blank")))
    } else {
        Ok(value.to_owned())
    }
}

fn normalize_list(values: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() || normalized.iter().any(|existing| existing == value) {
            continue;
        }
        normalized.push(value.to_owned());
    }
    normalized
}

fn write_document(path: &Path, document: &TextRulesDocument) -> Result<(), TextRulesError> {
    let parent = path.parent().ok_or_else(|| {
        TextRulesError::new(format!("text-rules path {} has no parent directory", path.display()))
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        TextRulesError::new(format!(
            "could not create text-rules directory {}: {error}",
            parent.display()
        ))
    })?;

    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(document)
        .map_err(|error| TextRulesError::new(format!("could not encode text rules: {error}")))?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| {
            TextRulesError::new(format!(
                "could not open temporary text-rules file {}: {error}",
                temporary.display()
            ))
        })?;
    file.write_all(&bytes).map_err(|error| {
        TextRulesError::new(format!(
            "could not write temporary text-rules file {}: {error}",
            temporary.display()
        ))
    })?;
    file.write_all(b"\n").map_err(|error| {
        TextRulesError::new(format!(
            "could not finalize temporary text-rules file {}: {error}",
            temporary.display()
        ))
    })?;
    file.sync_all().map_err(|error| {
        TextRulesError::new(format!(
            "could not sync temporary text-rules file {}: {error}",
            temporary.display()
        ))
    })?;
    drop(file);

    #[cfg(target_os = "windows")]
    if path.exists() {
        fs::remove_file(path).map_err(|error| {
            TextRulesError::new(format!(
                "could not replace existing text-rules file {}: {error}",
                path.display()
            ))
        })?;
    }

    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        TextRulesError::new(format!(
            "could not commit text-rules file {}: {error}",
            path.display()
        ))
    })
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
            "blcvoice-text-rules-{test_name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn replacement_is_case_insensitive_and_respects_word_boundaries() {
        let document = TextRulesDocument {
            replacements: vec![ReplacementRule {
                id: 1,
                from: "paysa".to_owned(),
                to: "yapay zeka".to_owned(),
                enabled: true,
                whole_word: true,
            }],
            ..TextRulesDocument::default()
        };
        assert_eq!(
            apply_document("Paysa güzel ama paysalık değil.", &document),
            "yapay zeka güzel ama paysalık değil."
        );
    }

    #[test]
    fn dictionary_aliases_preserve_turkish_and_emoji_text() {
        let document = TextRulesDocument {
            dictionary: vec![DictionaryEntry {
                id: 1,
                term: "BLCVoice".to_owned(),
                aliases: vec!["blc voice".to_owned()],
                enabled: true,
            }],
            ..TextRulesDocument::default()
        };
        assert_eq!(
            apply_document("Merhaba blc voice, nasılsın? 👋 Şimdi Türkçe.", &document),
            "Merhaba BLCVoice, nasılsın? 👋 Şimdi Türkçe."
        );
    }

    #[test]
    fn disabled_rules_are_ignored() {
        let document = TextRulesDocument {
            replacements: vec![ReplacementRule {
                id: 1,
                from: "yanlış".to_owned(),
                to: "doğru".to_owned(),
                enabled: false,
                whole_word: true,
            }],
            ..TextRulesDocument::default()
        };
        assert_eq!(apply_document("yanlış", &document), "yanlış");
    }

    #[test]
    fn service_round_trips_dictionary_and_replacements() {
        let directory = temporary_directory("round-trip");
        let service = TextRulesService::open(&directory).expect("service must open");
        service
            .save_dictionary(
                None,
                "OpenAI".to_owned(),
                vec!["open ai".to_owned()],
                None,
            )
            .expect("dictionary entry must save");
        service
            .save_replacement(
                None,
                "Paysa".to_owned(),
                "yapay zeka".to_owned(),
                None,
                None,
            )
            .expect("replacement must save");

        let reopened = TextRulesService::open(&directory).expect("service must reopen");
        let snapshot = reopened.snapshot();
        assert_eq!(snapshot.dictionary.len(), 1);
        assert_eq!(snapshot.replacements.len(), 1);
        assert_eq!(reopened.apply("open ai ve PAYSA"), "OpenAI ve yapay zeka");
        let _ = fs::remove_dir_all(directory);
    }
}
