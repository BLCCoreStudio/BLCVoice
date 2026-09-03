#![cfg(target_os = "linux")]

use blcvoice_insertion::{InsertionBackend, TextInserter};
use blcvoice_insertion_x11::{X11Inserter, X11Options};

#[test]
#[ignore = "requires a live X11 server with XTEST"]
fn live_x11_accepts_unicode_submission_and_restoration() {
    let mut inserter = X11Inserter::connect(X11Options::default())
        .expect("Xvfb should expose an X11 server with XTEST and a scratch keycode");
    let text = "BLCVoice Türkçe: çğıöşü İ🙂\n";

    let receipt = inserter
        .insert_text(text)
        .expect("X11 should accept the complete Unicode XTEST sequence and restore the keymap");

    assert_eq!(receipt.backend(), InsertionBackend::X11XTest);
    assert_eq!(receipt.submitted_utf8_bytes(), text.len());
    assert!(!receipt.semantic_delivery_verified());
}
