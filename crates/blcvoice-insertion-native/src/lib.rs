#![forbid(unsafe_code)]

use blcvoice_insertion::{
    InsertionAuthorization, InsertionBackend, InsertionCapability, InsertionError,
    InsertionErrorKind, InsertionReceipt, TextInserter,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeInsertionOptions {
    prompt_for_accessibility: bool,
}

impl NativeInsertionOptions {
    #[must_use]
    pub const fn new(prompt_for_accessibility: bool) -> Self {
        Self {
            prompt_for_accessibility,
        }
    }

    #[must_use]
    pub const fn prompt_for_accessibility(&self) -> bool {
        self.prompt_for_accessibility
    }
}

impl Default for NativeInsertionOptions {
    fn default() -> Self {
        Self::new(true)
    }
}

fn validate_text(text: &str) -> Result<(), InsertionError> {
    if text.is_empty() {
        return Err(InsertionError::new(
            InsertionErrorKind::InvalidText,
            "cannot submit empty text through the native insertion backend",
        ));
    }
    if text.contains('\0') {
        return Err(InsertionError::new(
            InsertionErrorKind::InvalidText,
            "text contains a NUL byte, which the native insertion backend cannot encode",
        ));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
const fn native_capability() -> InsertionCapability {
    InsertionCapability::new(InsertionBackend::WindowsSendInput, InsertionAuthorization::None)
}

#[cfg(target_os = "macos")]
const fn native_capability() -> InsertionCapability {
    InsertionCapability::new(
        InsertionBackend::MacOsQuartz,
        InsertionAuthorization::MacOsAccessibility,
    )
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const fn native_capability() -> InsertionCapability {
    InsertionCapability::new(InsertionBackend::WindowsSendInput, InsertionAuthorization::None)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod platform {
    use enigo::{Enigo, Keyboard, NewConError, Settings};

    use super::{
        InsertionError, InsertionErrorKind, InsertionReceipt, NativeInsertionOptions, TextInserter,
        native_capability, validate_text,
    };

    pub struct NativeInserter {
        enigo: Enigo,
    }

    impl NativeInserter {
        pub fn connect(options: NativeInsertionOptions) -> Result<Self, InsertionError> {
            let settings = Settings {
                open_prompt_to_get_permissions: options.prompt_for_accessibility(),
                ..Settings::default()
            };
            let enigo = Enigo::new(&settings).map_err(map_connection_error)?;
            Ok(Self { enigo })
        }
    }

    impl TextInserter for NativeInserter {
        fn capability(&self) -> super::InsertionCapability {
            native_capability()
        }

        fn insert_text(&mut self, text: &str) -> Result<InsertionReceipt, InsertionError> {
            validate_text(text)?;
            self.enigo.text(text).map_err(|error| {
                InsertionError::new(
                    InsertionErrorKind::BackendFailure,
                    format!("native text submission failed: {error}"),
                )
            })?;
            Ok(InsertionReceipt::complete(
                native_capability().backend(),
                text.len(),
            ))
        }
    }

    fn map_connection_error(error: NewConError) -> InsertionError {
        let kind = if matches!(error, NewConError::NoPermission) {
            InsertionErrorKind::PermissionDenied
        } else {
            InsertionErrorKind::BackendUnavailable
        };
        InsertionError::new(kind, format!("native insertion backend is unavailable: {error}"))
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod platform {
    use super::{
        InsertionError, InsertionErrorKind, InsertionReceipt, NativeInsertionOptions, TextInserter,
        native_capability, validate_text,
    };

    pub struct NativeInserter;

    impl NativeInserter {
        pub fn connect(_options: NativeInsertionOptions) -> Result<Self, InsertionError> {
            Err(InsertionError::new(
                InsertionErrorKind::BackendUnavailable,
                "the native SendInput/Quartz adapter is available only on Windows and macOS",
            ))
        }
    }

    impl TextInserter for NativeInserter {
        fn capability(&self) -> super::InsertionCapability {
            native_capability()
        }

        fn insert_text(&mut self, text: &str) -> Result<InsertionReceipt, InsertionError> {
            validate_text(text)?;
            Err(InsertionError::new(
                InsertionErrorKind::BackendUnavailable,
                "the native SendInput/Quartz adapter is available only on Windows and macOS",
            ))
        }
    }
}

pub use platform::NativeInserter;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_rejects_empty_and_nul_text() {
        assert_eq!(
            validate_text("").expect_err("empty text must fail").kind(),
            InsertionErrorKind::InvalidText
        );
        assert_eq!(
            validate_text("hello\0world")
                .expect_err("NUL text must fail")
                .kind(),
            InsertionErrorKind::InvalidText
        );
        validate_text("Türkçe 🙂").expect("Unicode text must be accepted");
    }

    #[test]
    fn accessibility_prompt_defaults_on() {
        assert!(NativeInsertionOptions::default().prompt_for_accessibility());
        assert!(!NativeInsertionOptions::new(false).prompt_for_accessibility());
    }
}
