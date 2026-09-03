use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, TryLockError};
use std::time::Duration;

use blcvoice_asr_transcribe::{TranscribeRecognizer, TranscribeRecognizerConfig};
use reqwest::blocking::Client;
use serde::Serialize;
use sysinfo::System;

const GIB: u64 = 1024 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const DOWNLOAD_SLACK_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelTier {
    Fast,
    Balanced,
    Quality,
}

impl ModelTier {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Balanced => "balanced",
            Self::Quality => "quality",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelSpec {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    tier: ModelTier,
    filename: &'static str,
    url: &'static str,
    advertised_bytes: u64,
}

impl ModelSpec {
    #[must_use]
    pub const fn id(self) -> &'static str {
        self.id
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }

    #[must_use]
    pub const fn description(self) -> &'static str {
        self.description
    }

    #[must_use]
    pub const fn tier(self) -> ModelTier {
        self.tier
    }

    #[must_use]
    pub const fn advertised_bytes(self) -> u64 {
        self.advertised_bytes
    }

    fn maximum_download_bytes(self) -> u64 {
        self.advertised_bytes + DOWNLOAD_SLACK_BYTES
    }
}

pub const MODEL_CATALOG: [ModelSpec; 3] = [
    ModelSpec {
        id: "whisper-base-q5km",
        name: "Whisper Base Q5",
        description: "Lowest memory footprint for fast local dictation.",
        tier: ModelTier::Fast,
        filename: "whisper-base-Q5_K_M.gguf",
        url: "https://huggingface.co/handy-computer/whisper-base-gguf/resolve/main/whisper-base-Q5_K_M.gguf",
        advertised_bytes: 61 * 1024 * 1024,
    },
    ModelSpec {
        id: "whisper-small-q5km",
        name: "Whisper Small Q5",
        description: "Balanced accuracy and resource use; recommended for most systems.",
        tier: ModelTier::Balanced,
        filename: "whisper-small-Q5_K_M.gguf",
        url: "https://huggingface.co/handy-computer/whisper-small-gguf/resolve/main/whisper-small-Q5_K_M.gguf",
        advertised_bytes: 185 * 1024 * 1024,
    },
    ModelSpec {
        id: "whisper-large-v3-turbo-q4km",
        name: "Whisper Large v3 Turbo Q4",
        description: "Higher-quality local transcription for systems with more memory.",
        tier: ModelTier::Quality,
        filename: "whisper-large-v3-turbo-Q4_K_M.gguf",
        url: "https://huggingface.co/handy-computer/whisper-large-v3-turbo-gguf/resolve/main/whisper-large-v3-turbo-Q4_K_M.gguf",
        advertised_bytes: 511 * 1024 * 1024,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelErrorKind {
    UnknownModel,
    Busy,
    Network,
    DownloadInvalid,
    Validation,
    Io,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelError {
    kind: ModelErrorKind,
    message: String,
}

impl ModelError {
    fn new(kind: ModelErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> ModelErrorKind {
        self.kind
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ModelError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelStatus {
    pub spec: ModelSpec,
    pub installed: bool,
    pub installed_bytes: Option<u64>,
    pub recommended: bool,
}

#[derive(Debug)]
pub struct ModelManager {
    root: PathBuf,
    client: Client,
    install_lock: Mutex<()>,
}

impl ModelManager {
    pub fn new(data_dir: impl Into<PathBuf>) -> Result<Self, ModelError> {
        let root = data_dir.into().join("models");
        fs::create_dir_all(&root).map_err(|error| {
            ModelError::new(
                ModelErrorKind::Io,
                format!("could not create model directory {}: {error}", root.display()),
            )
        })?;
        let client = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(DOWNLOAD_TIMEOUT)
            .user_agent(concat!("BLCVoice/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| {
                ModelError::new(
                    ModelErrorKind::Network,
                    format!("could not initialize model download client: {error}"),
                )
            })?;
        Ok(Self {
            root,
            client,
            install_lock: Mutex::new(()),
        })
    }

    #[must_use]
    pub fn catalog(&self) -> Vec<ModelStatus> {
        let recommended = self.recommended_model_id();
        MODEL_CATALOG
            .iter()
            .copied()
            .map(|spec| {
                let path = self.model_path(spec);
                let installed_bytes = fs::metadata(path).ok().map(|metadata| metadata.len());
                ModelStatus {
                    spec,
                    installed: installed_bytes.is_some(),
                    installed_bytes,
                    recommended: recommended == spec.id(),
                }
            })
            .collect()
    }

    #[must_use]
    pub fn recommended_model_id(&self) -> &'static str {
        recommend_for_memory(System::new_all().total_memory())
    }

    pub fn installed_model_path(&self, id: &str) -> Result<Option<PathBuf>, ModelError> {
        let spec = find_model(id)?;
        let path = self.model_path(spec);
        Ok(path.is_file().then_some(path))
    }

    pub fn install(&self, id: &str) -> Result<ModelStatus, ModelError> {
        let spec = find_model(id)?;
        let _guard = match self.install_lock.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::WouldBlock) => {
                return Err(ModelError::new(
                    ModelErrorKind::Busy,
                    "another model operation is already in progress",
                ));
            }
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        };

        let final_path = self.model_path(spec);
        if final_path.is_file() {
            return Ok(self.status_for(spec));
        }

        let temporary_path = self.root.join(format!("{}.part", spec.filename));
        let _ = fs::remove_file(&temporary_path);
        let result = self.download_and_validate(spec, &temporary_path, &final_path);
        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result?;
        Ok(self.status_for(spec))
    }

    pub fn remove(&self, id: &str) -> Result<ModelStatus, ModelError> {
        let spec = find_model(id)?;
        let _guard = match self.install_lock.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::WouldBlock) => {
                return Err(ModelError::new(
                    ModelErrorKind::Busy,
                    "another model operation is already in progress",
                ));
            }
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        };
        let path = self.model_path(spec);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(ModelError::new(
                    ModelErrorKind::Io,
                    format!("could not remove model {}: {error}", path.display()),
                ));
            }
        }
        Ok(self.status_for(spec))
    }

    fn download_and_validate(
        &self,
        spec: ModelSpec,
        temporary_path: &Path,
        final_path: &Path,
    ) -> Result<(), ModelError> {
        let mut response = self
            .client
            .get(spec.url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|error| {
                ModelError::new(
                    ModelErrorKind::Network,
                    format!("could not download {}: {error}", spec.name),
                )
            })?;

        if response
            .content_length()
            .is_some_and(|length| length > spec.maximum_download_bytes())
        {
            return Err(ModelError::new(
                ModelErrorKind::DownloadInvalid,
                format!("{} download is unexpectedly large", spec.name),
            ));
        }

        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(temporary_path)
            .map_err(|error| {
                ModelError::new(
                    ModelErrorKind::Io,
                    format!(
                        "could not create temporary model file {}: {error}",
                        temporary_path.display()
                    ),
                )
            })?;
        let expected_length = response.content_length();
        let mut buffer = [0_u8; 64 * 1024];
        let mut downloaded = 0_u64;
        loop {
            let read = response.read(&mut buffer).map_err(|error| {
                ModelError::new(
                    ModelErrorKind::Network,
                    format!("model download stream failed: {error}"),
                )
            })?;
            if read == 0 {
                break;
            }
            downloaded = downloaded.saturating_add(read as u64);
            if downloaded > spec.maximum_download_bytes() {
                return Err(ModelError::new(
                    ModelErrorKind::DownloadInvalid,
                    format!("{} download exceeded its safety limit", spec.name),
                ));
            }
            file.write_all(&buffer[..read]).map_err(|error| {
                ModelError::new(
                    ModelErrorKind::Io,
                    format!("could not write downloaded model: {error}"),
                )
            })?;
        }
        file.sync_all().map_err(|error| {
            ModelError::new(
                ModelErrorKind::Io,
                format!("could not sync downloaded model: {error}"),
            )
        })?;
        drop(file);

        if expected_length.is_some_and(|length| length != downloaded) {
            return Err(ModelError::new(
                ModelErrorKind::DownloadInvalid,
                format!(
                    "{} download was incomplete: expected {} bytes, received {}",
                    spec.name,
                    expected_length.unwrap_or_default(),
                    downloaded
                ),
            ));
        }
        if downloaded < spec.advertised_bytes.saturating_sub(DOWNLOAD_SLACK_BYTES) {
            return Err(ModelError::new(
                ModelErrorKind::DownloadInvalid,
                format!("{} download is unexpectedly small", spec.name),
            ));
        }

        TranscribeRecognizer::load(temporary_path, TranscribeRecognizerConfig::default())
            .map_err(|error| {
                ModelError::new(
                    ModelErrorKind::Validation,
                    format!("downloaded {} model failed validation: {error}", spec.name),
                )
            })?;

        #[cfg(target_os = "windows")]
        if final_path.exists() {
            fs::remove_file(final_path).map_err(|error| {
                ModelError::new(
                    ModelErrorKind::Io,
                    format!("could not replace existing model: {error}"),
                )
            })?;
        }
        fs::rename(temporary_path, final_path).map_err(|error| {
            ModelError::new(
                ModelErrorKind::Io,
                format!("could not commit downloaded model: {error}"),
            )
        })
    }

    fn model_path(&self, spec: ModelSpec) -> PathBuf {
        self.root.join(spec.filename)
    }

    fn status_for(&self, spec: ModelSpec) -> ModelStatus {
        let path = self.model_path(spec);
        let installed_bytes = fs::metadata(path).ok().map(|metadata| metadata.len());
        ModelStatus {
            spec,
            installed: installed_bytes.is_some(),
            installed_bytes,
            recommended: self.recommended_model_id() == spec.id(),
        }
    }
}

pub fn find_model(id: &str) -> Result<ModelSpec, ModelError> {
    MODEL_CATALOG
        .iter()
        .copied()
        .find(|spec| spec.id == id)
        .ok_or_else(|| {
            ModelError::new(
                ModelErrorKind::UnknownModel,
                format!("unknown BLCVoice model id {id}"),
            )
        })
}

#[must_use]
pub const fn recommend_for_memory(total_memory_bytes: u64) -> &'static str {
    if total_memory_bytes < 8 * GIB {
        "whisper-base-q5km"
    } else if total_memory_bytes < 16 * GIB {
        "whisper-small-q5km"
    } else {
        "whisper-large-v3-turbo-q4km"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommendation_scales_with_system_memory() {
        assert_eq!(recommend_for_memory(4 * GIB), "whisper-base-q5km");
        assert_eq!(recommend_for_memory(8 * GIB), "whisper-small-q5km");
        assert_eq!(recommend_for_memory(12 * GIB), "whisper-small-q5km");
        assert_eq!(
            recommend_for_memory(16 * GIB),
            "whisper-large-v3-turbo-q4km"
        );
    }

    #[test]
    fn catalog_ids_and_filenames_are_unique() {
        for (index, model) in MODEL_CATALOG.iter().enumerate() {
            assert!(!model.id.is_empty());
            assert!(model.filename.ends_with(".gguf"));
            assert!(model.url.starts_with("https://huggingface.co/handy-computer/"));
            for other in MODEL_CATALOG.iter().skip(index + 1) {
                assert_ne!(model.id, other.id);
                assert_ne!(model.filename, other.filename);
            }
        }
    }

    #[test]
    fn unknown_model_is_rejected() {
        let error = find_model("not-a-model").expect_err("unknown model must fail");
        assert_eq!(error.kind(), ModelErrorKind::UnknownModel);
    }
}
