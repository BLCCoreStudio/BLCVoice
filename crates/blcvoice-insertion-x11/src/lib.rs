#![forbid(unsafe_code)]

use blcvoice_insertion::{
    InsertionAuthorization, InsertionBackend, InsertionCapability, InsertionError,
    InsertionErrorKind, InsertionReceipt, TextInserter,
};

const XK_RETURN: u32 = 0xFF0D;
const XK_TAB: u32 = 0xFF09;
const UNICODE_KEYSYM_PREFIX: u32 = 0x0100_0000;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct X11Options {
    display: Option<String>,
}

impl X11Options {
    #[must_use]
    pub fn new(display: Option<String>) -> Self {
        Self { display }
    }

    #[must_use]
    pub fn display(&self) -> Option<&str> {
        self.display.as_deref()
    }
}

const fn x11_capability() -> InsertionCapability {
    InsertionCapability::new(InsertionBackend::X11XTest, InsertionAuthorization::None)
}

fn validate_text(text: &str) -> Result<(), InsertionError> {
    if text.is_empty() {
        return Err(InsertionError::new(
            InsertionErrorKind::InvalidText,
            "cannot submit empty text through XTEST",
        ));
    }

    for character in text.chars() {
        if character == '\0' {
            return Err(InsertionError::new(
                InsertionErrorKind::InvalidText,
                "text contains a NUL character",
            ));
        }
        if character.is_control() && !matches!(character, '\n' | '\r' | '\t') {
            return Err(InsertionError::new(
                InsertionErrorKind::InvalidText,
                format!(
                    "text contains unsupported control character U+{:04X}",
                    character as u32
                ),
            ));
        }
    }

    Ok(())
}

fn char_to_keysym(character: char) -> u32 {
    match character {
        '\n' | '\r' => XK_RETURN,
        '\t' => XK_TAB,
        character if matches!(character as u32, 0x20..=0x7E | 0xA0..=0xFF) => character as u32,
        character => UNICODE_KEYSYM_PREFIX | character as u32,
    }
}

fn find_scratch_keycode(
    min_keycode: u8,
    keysyms_per_keycode: usize,
    keysyms: &[u32],
    modifier_keycodes: &[u8],
) -> Option<u8> {
    if keysyms_per_keycode == 0 {
        return None;
    }

    keysyms
        .chunks_exact(keysyms_per_keycode)
        .enumerate()
        .find_map(|(index, mapping)| {
            let keycode = min_keycode.checked_add(u8::try_from(index).ok()?)?;
            let unused = mapping.iter().all(|keysym| *keysym == 0);
            let modifier = modifier_keycodes.contains(&keycode);
            (unused && !modifier).then_some(keycode)
        })
}

#[cfg(target_os = "linux")]
mod platform {
    use x11rb::{
        CURRENT_TIME, NONE,
        connection::Connection,
        protocol::{
            xproto::{
                ConnectionExt as XprotoConnectionExt, KEY_PRESS_EVENT, KEY_RELEASE_EVENT,
            },
            xtest::ConnectionExt as XtestConnectionExt,
        },
        rust_connection::RustConnection,
    };

    use super::{
        InsertionBackend, InsertionError, InsertionErrorKind, InsertionReceipt, TextInserter,
        X11Options, char_to_keysym, find_scratch_keycode, validate_text, x11_capability,
    };

    struct ScratchMapping {
        keycode: u8,
        keysyms_per_keycode: u8,
        original: Vec<u32>,
    }

    pub struct X11Inserter {
        connection: RustConnection,
        scratch: ScratchMapping,
    }

    impl X11Inserter {
        pub fn connect(options: X11Options) -> Result<Self, InsertionError> {
            let (connection, _screen_number) =
                x11rb::connect(options.display()).map_err(|error| {
                    unavailable(format!("failed to connect to the X11 server: {error}"))
                })?;

            let xtest = connection
                .query_extension(b"XTEST")
                .map_err(|error| unavailable(format!("failed to query XTEST: {error}")))?
                .reply()
                .map_err(|error| unavailable(format!("failed to read XTEST capability: {error}")))?;
            if !xtest.present {
                return Err(unavailable(
                    "the active X11 server does not expose the XTEST extension",
                ));
            }

            let setup = connection.setup();
            let min_keycode = setup.min_keycode;
            let max_keycode = setup.max_keycode;
            let count_u16 = u16::from(max_keycode)
                .saturating_sub(u16::from(min_keycode))
                .saturating_add(1);
            let count = u8::try_from(count_u16).map_err(|_| {
                unavailable("the X11 keycode range cannot be represented safely")
            })?;

            let mapping = connection
                .get_keyboard_mapping(min_keycode, count)
                .map_err(|error| unavailable(format!("failed to request X11 keymap: {error}")))?
                .reply()
                .map_err(|error| unavailable(format!("failed to read X11 keymap: {error}")))?;
            let keysyms_per_keycode = mapping.keysyms_per_keycode;
            if keysyms_per_keycode == 0 {
                return Err(unavailable(
                    "the X11 server returned an empty keyboard mapping width",
                ));
            }

            let modifiers = connection
                .get_modifier_mapping()
                .map_err(|error| {
                    unavailable(format!("failed to request X11 modifier map: {error}"))
                })?
                .reply()
                .map_err(|error| {
                    unavailable(format!("failed to read X11 modifier map: {error}"))
                })?;

            let scratch_keycode = find_scratch_keycode(
                min_keycode,
                usize::from(keysyms_per_keycode),
                &mapping.keysyms,
                &modifiers.keycodes,
            )
            .ok_or_else(|| {
                unavailable(
                    "no unused non-modifier X11 keycode is available for layout-independent text insertion",
                )
            })?;

            let offset = usize::from(scratch_keycode - min_keycode)
                .saturating_mul(usize::from(keysyms_per_keycode));
            let end = offset.saturating_add(usize::from(keysyms_per_keycode));
            let original = mapping
                .keysyms
                .get(offset..end)
                .ok_or_else(|| unavailable("the selected X11 scratch keycode mapping is invalid"))?
                .to_vec();

            Ok(Self {
                connection,
                scratch: ScratchMapping {
                    keycode: scratch_keycode,
                    keysyms_per_keycode,
                    original,
                },
            })
        }

        fn apply_keysym(&self, keysym: u32) -> Result<(), InsertionError> {
            let mut mapping = vec![0; usize::from(self.scratch.keysyms_per_keycode)];
            mapping[0] = keysym;

            self.connection
                .change_keyboard_mapping(
                    1,
                    self.scratch.keycode,
                    self.scratch.keysyms_per_keycode,
                    &mapping,
                )
                .map_err(|error| backend_failure(format!("failed to remap X11 scratch key: {error}")))?
                .check()
                .map_err(|error| {
                    backend_failure(format!("X11 rejected scratch key remapping: {error}"))
                })
        }

        fn emit_key(&self) -> Result<(), InsertionError> {
            self.connection
                .xtest_fake_input(
                    KEY_PRESS_EVENT,
                    self.scratch.keycode,
                    CURRENT_TIME,
                    NONE,
                    0,
                    0,
                    0,
                )
                .map_err(|error| backend_failure(format!("failed to queue XTEST key press: {error}")))?
                .check()
                .map_err(|error| backend_failure(format!("XTEST key press failed: {error}")))?;

            self.connection
                .xtest_fake_input(
                    KEY_RELEASE_EVENT,
                    self.scratch.keycode,
                    CURRENT_TIME,
                    NONE,
                    0,
                    0,
                    0,
                )
                .map_err(|error| {
                    backend_failure(format!("failed to queue XTEST key release: {error}"))
                })?
                .check()
                .map_err(|error| backend_failure(format!("XTEST key release failed: {error}")))?;

            self.connection
                .flush()
                .map_err(|error| backend_failure(format!("failed to flush X11 input: {error}")))
        }

        fn restore_mapping(&self) -> Result<(), InsertionError> {
            self.connection
                .change_keyboard_mapping(
                    1,
                    self.scratch.keycode,
                    self.scratch.keysyms_per_keycode,
                    &self.scratch.original,
                )
                .map_err(|error| {
                    backend_failure(format!("failed to queue X11 keymap restoration: {error}"))
                })?
                .check()
                .map_err(|error| {
                    backend_failure(format!("X11 rejected keymap restoration: {error}"))
                })?;
            self.connection.flush().map_err(|error| {
                backend_failure(format!("failed to flush X11 keymap restoration: {error}"))
            })
        }

        fn insert_inner(&self, text: &str) -> Result<usize, (usize, InsertionError)> {
            let mut submitted = 0usize;
            let mut active_keysym = None;

            for character in text.chars() {
                let keysym = char_to_keysym(character);
                if active_keysym != Some(keysym) {
                    if let Err(error) = self.apply_keysym(keysym) {
                        return Err((submitted, error));
                    }
                    active_keysym = Some(keysym);
                }

                if let Err(error) = self.emit_key() {
                    return Err((submitted, error));
                }
                submitted = submitted.saturating_add(character.len_utf8());
            }

            Ok(submitted)
        }
    }

    impl TextInserter for X11Inserter {
        fn capability(&self) -> super::InsertionCapability {
            x11_capability()
        }

        fn insert_text(&mut self, text: &str) -> Result<InsertionReceipt, InsertionError> {
            validate_text(text)?;

            let insertion = self.insert_inner(text);
            let restoration = self.restore_mapping();

            match (insertion, restoration) {
                (Ok(submitted), Ok(())) => Ok(InsertionReceipt::complete(
                    InsertionBackend::X11XTest,
                    submitted,
                )),
                (Ok(_), Err(error)) => Err(InsertionError::new(
                    InsertionErrorKind::BackendFailure,
                    format!(
                        "text was submitted through XTEST, but the scratch key mapping could not be restored: {error}"
                    ),
                )),
                (Err((submitted, error)), Ok(())) => Err(submission_error(submitted, error)),
                (Err((submitted, error)), Err(restore_error)) => {
                    let kind = if submitted == 0 {
                        InsertionErrorKind::BackendFailure
                    } else {
                        InsertionErrorKind::PartialSubmission
                    };
                    Err(InsertionError::new(
                        kind,
                        format!(
                            "XTEST insertion failed after {submitted} UTF-8 bytes and keymap restoration also failed; insertion error: {error}; restoration error: {restore_error}"
                        ),
                    ))
                }
            }
        }
    }

    impl Drop for X11Inserter {
        fn drop(&mut self) {
            let _ = self.restore_mapping();
        }
    }

    fn submission_error(submitted: usize, error: InsertionError) -> InsertionError {
        let kind = if submitted == 0 {
            error.kind()
        } else {
            InsertionErrorKind::PartialSubmission
        };
        InsertionError::new(
            kind,
            format!(
                "XTEST insertion failed after {submitted} UTF-8 bytes were submitted: {error}"
            ),
        )
    }

    fn unavailable(message: impl Into<String>) -> InsertionError {
        InsertionError::new(InsertionErrorKind::BackendUnavailable, message)
    }

    fn backend_failure(message: impl Into<String>) -> InsertionError {
        InsertionError::new(InsertionErrorKind::BackendFailure, message)
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use super::{
        InsertionError, InsertionErrorKind, InsertionReceipt, TextInserter, X11Options,
        validate_text, x11_capability,
    };

    pub struct X11Inserter;

    impl X11Inserter {
        pub fn connect(_options: X11Options) -> Result<Self, InsertionError> {
            Err(InsertionError::new(
                InsertionErrorKind::BackendUnavailable,
                "the X11 XTEST adapter is available only on Linux",
            ))
        }
    }

    impl TextInserter for X11Inserter {
        fn capability(&self) -> super::InsertionCapability {
            x11_capability()
        }

        fn insert_text(&mut self, text: &str) -> Result<InsertionReceipt, InsertionError> {
            validate_text(text)?;
            Err(InsertionError::new(
                InsertionErrorKind::BackendUnavailable,
                "the X11 XTEST adapter is available only on Linux",
            ))
        }
    }
}

pub use platform::X11Inserter;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_keysyms_follow_x11_encoding_rules() {
        assert_eq!(char_to_keysym('A'), 0x41);
        assert_eq!(char_to_keysym('é'), 0xE9);
        assert_eq!(char_to_keysym('ğ'), 0x0100_011F);
        assert_eq!(char_to_keysym('İ'), 0x0100_0130);
        assert_eq!(char_to_keysym('🙂'), 0x0101_F642);
        assert_eq!(char_to_keysym('\n'), XK_RETURN);
        assert_eq!(char_to_keysym('\t'), XK_TAB);
    }

    #[test]
    fn validation_rejects_empty_nul_and_unsupported_controls() {
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
        assert_eq!(
            validate_text("hello\u{7}world")
                .expect_err("bell control must fail")
                .kind(),
            InsertionErrorKind::InvalidText
        );
        validate_text("Türkçe\nikinci satır\tson")
            .expect("dictation text controls should be accepted");
    }

    #[test]
    fn scratch_keycode_must_be_empty_and_not_a_modifier() {
        let keysyms = [
            1, 0, // keycode 8 is occupied
            0, 0, // keycode 9 is empty but modifier
            0, 0, // keycode 10 is safe
        ];
        assert_eq!(find_scratch_keycode(8, 2, &keysyms, &[9]), Some(10));
        assert_eq!(find_scratch_keycode(8, 2, &keysyms, &[9, 10]), None);
    }

    #[test]
    fn capability_is_explicitly_x11_xtest() {
        let capability = x11_capability();
        assert_eq!(capability.backend(), InsertionBackend::X11XTest);
        assert_eq!(capability.authorization(), InsertionAuthorization::None);
        assert!(!capability.semantic_delivery_verifiable());
    }
}
