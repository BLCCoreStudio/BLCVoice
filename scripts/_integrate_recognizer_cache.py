from pathlib import Path

path = Path("apps/desktop/src-tauri/src/dictation.rs")
s = path.read_text()

s = s.replace(
    "use std::fmt;\nuse std::mem;\nuse std::path::{Path, PathBuf};\nuse std::sync::{Arc, Mutex, MutexGuard};\n",
    "use std::fmt;\nuse std::fs;\nuse std::mem;\nuse std::path::{Path, PathBuf};\nuse std::sync::{Arc, Mutex, MutexGuard};\nuse std::time::SystemTime;\n",
    1,
)

needle = '''impl RecognizerFactory for TranscribeRecognizerFactory {
    fn load(&self, model_path: &Path) -> Result<Box<dyn SpeechRecognizer>, RecognitionError> {
        TranscribeRecognizer::load(model_path, TranscribeRecognizerConfig::default())
            .map(|recognizer| Box::new(recognizer) as Box<dyn SpeechRecognizer>)
    }
}

'''
addition = needle + '''#[derive(Debug, Clone, PartialEq, Eq)]
struct RecognizerCacheKey {
    model_path: PathBuf,
    file_len: Option<u64>,
    modified: Option<SystemTime>,
}

impl RecognizerCacheKey {
    fn for_path(model_path: &Path) -> Self {
        let resolved_path = fs::canonicalize(model_path).unwrap_or_else(|_| model_path.to_path_buf());
        let metadata = fs::metadata(&resolved_path).ok();
        Self {
            model_path: resolved_path,
            file_len: metadata.as_ref().map(fs::Metadata::len),
            modified: metadata.and_then(|metadata| metadata.modified().ok()),
        }
    }
}

struct CachedRecognizer {
    key: RecognizerCacheKey,
    recognizer: Box<dyn SpeechRecognizer>,
}

'''
if needle not in s:
    raise SystemExit("factory block not found")
s = s.replace(needle, addition, 1)

s = s.replace(
    '''struct ActiveDictation {
    session_id: SessionId,
    recognizer: Box<dyn SpeechRecognizer>,
    recognition: RecognitionOptions,
}
''',
    '''struct ActiveDictation {
    session_id: SessionId,
    recognizer_key: RecognizerCacheKey,
    recognizer: Box<dyn SpeechRecognizer>,
    recognition: RecognitionOptions,
}
''',
    1,
)

s = s.replace(
    '''pub struct DesktopDictationService {
    capture: Arc<DesktopCaptureService>,
    recognizers: Arc<dyn RecognizerFactory>,
    slot: Mutex<DictationSlot>,
}
''',
    '''pub struct DesktopDictationService {
    capture: Arc<DesktopCaptureService>,
    recognizers: Arc<dyn RecognizerFactory>,
    recognizer_cache: Mutex<Option<CachedRecognizer>>,
    slot: Mutex<DictationSlot>,
}
''',
    1,
)

s = s.replace(
    '''        Self {
            capture,
            recognizers,
            slot: Mutex::new(DictationSlot::Idle),
        }
''',
    '''        Self {
            capture,
            recognizers,
            recognizer_cache: Mutex::new(None),
            slot: Mutex::new(DictationSlot::Idle),
        }
''',
    1,
)

s = s.replace(
    '''        let recognizer = match self.recognizers.load(&request.model_path) {
            Ok(recognizer) => recognizer,
            Err(error) => {
                self.reset_to_idle();
                return Err(DesktopDictationError::new(
                    DesktopDictationErrorKind::RecognizerLoad,
                    format!("could not load dictation model: {error}"),
                ));
            }
        };

        let session = match self
''',
    '''        let (recognizer_key, recognizer) = match self.acquire_recognizer(&request.model_path) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.reset_to_idle();
                return Err(DesktopDictationError::new(
                    DesktopDictationErrorKind::RecognizerLoad,
                    format!("could not load dictation model: {error}"),
                ));
            }
        };

        let session = match self
''',
    1,
)

s = s.replace(
    '''            Err(error) => {
                self.reset_to_idle();
                return Err(map_capture_error(error));
            }
        };

        let mut slot = self.lock_slot();
''',
    '''            Err(error) => {
                self.recycle_recognizer(recognizer_key, recognizer);
                self.reset_to_idle();
                return Err(map_capture_error(error));
            }
        };

        let mut slot = self.lock_slot();
''',
    1,
)

s = s.replace(
    '''        if !matches!(*slot, DictationSlot::Preparing) {
            drop(slot);
            let _ = self.capture.cancel_dictation(session.id);
            self.reset_to_idle();
            return Err(DesktopDictationError::new(
''',
    '''        if !matches!(*slot, DictationSlot::Preparing) {
            drop(slot);
            self.recycle_recognizer(recognizer_key, recognizer);
            let _ = self.capture.cancel_dictation(session.id);
            self.reset_to_idle();
            return Err(DesktopDictationError::new(
''',
    1,
)

s = s.replace(
    '''        *slot = DictationSlot::Recording(ActiveDictation {
            session_id: session.id,
            recognizer,
            recognition: request.recognition,
        });
''',
    '''        *slot = DictationSlot::Recording(ActiveDictation {
            session_id: session.id,
            recognizer_key,
            recognizer,
            recognition: request.recognition,
        });
''',
    1,
)

s = s.replace(
    '''        let finalized = match self.capture.finish_dictation_recording(session_id) {
            Ok(finalized) => finalized,
            Err(error) => {
                self.reset_to_idle();
                return Err(map_capture_error(error));
            }
        };
''',
    '''        let finalized = match self.capture.finish_dictation_recording(session_id) {
            Ok(finalized) => finalized,
            Err(error) => {
                self.recycle_recognizer(active.recognizer_key, active.recognizer);
                self.reset_to_idle();
                return Err(map_capture_error(error));
            }
        };
''',
    1,
)

s = s.replace(
    '''        let transcription = match self.capture.transcribe_dictation(
            session_id,
            active.recognizer.as_mut(),
            &active.recognition,
        ) {
            Ok(transcription) => transcription,
            Err(error) => {
                let _ = self.capture.fail_dictation_recognition(session_id);
                self.reset_to_idle();
                return Err(DesktopDictationError::new(
                    DesktopDictationErrorKind::Transcription,
                    format!("dictation transcription failed: {error}"),
                ));
            }
        };
''',
    '''        let transcription_result = self.capture.transcribe_dictation(
            session_id,
            active.recognizer.as_mut(),
            &active.recognition,
        );
        self.recycle_recognizer(active.recognizer_key, active.recognizer);
        let transcription = match transcription_result {
            Ok(transcription) => transcription,
            Err(error) => {
                let _ = self.capture.fail_dictation_recognition(session_id);
                self.reset_to_idle();
                return Err(DesktopDictationError::new(
                    DesktopDictationErrorKind::Transcription,
                    format!("dictation transcription failed: {error}"),
                ));
            }
        };
''',
    1,
)

old_cancel = '''    pub fn cancel(&self, session_id: SessionId) -> Result<SessionSnapshot, DesktopDictationError> {
        {
            let mut slot = self.lock_slot();
            let current = mem::take(&mut *slot);
            match current {
                DictationSlot::Recording(active) if active.session_id == session_id => {}
                DictationSlot::AwaitingInsertion(active_id) if active_id == session_id => {}
                DictationSlot::Recording(active) => {
                    let active_id = active.session_id;
                    *slot = DictationSlot::Recording(active);
                    return Err(stale_error(session_id, active_id));
                }
                DictationSlot::AwaitingInsertion(active_id) => {
                    *slot = DictationSlot::AwaitingInsertion(active_id);
                    return Err(stale_error(session_id, active_id));
                }
                DictationSlot::Finalizing(active_id) => {
                    *slot = DictationSlot::Finalizing(active_id);
                    return Err(busy_error("finalizing"));
                }
                DictationSlot::Inserting(active_id) => {
                    *slot = DictationSlot::Inserting(active_id);
                    return Err(busy_error("inserting"));
                }
                DictationSlot::Preparing => {
                    *slot = DictationSlot::Preparing;
                    return Err(busy_error("preparing"));
                }
                DictationSlot::Idle => {
                    return Err(DesktopDictationError::new(
                        DesktopDictationErrorKind::Busy,
                        "there is no active dictation to cancel",
                    ));
                }
            }
        }

        let result = self
            .capture
            .cancel_dictation(session_id)
            .map_err(map_capture_error);
        self.reset_to_idle();
        result
    }
'''
new_cancel = '''    pub fn cancel(&self, session_id: SessionId) -> Result<SessionSnapshot, DesktopDictationError> {
        let recognizer_to_recycle = {
            let mut slot = self.lock_slot();
            let current = mem::take(&mut *slot);
            match current {
                DictationSlot::Recording(active) if active.session_id == session_id => {
                    Some((active.recognizer_key, active.recognizer))
                }
                DictationSlot::AwaitingInsertion(active_id) if active_id == session_id => None,
                DictationSlot::Recording(active) => {
                    let active_id = active.session_id;
                    *slot = DictationSlot::Recording(active);
                    return Err(stale_error(session_id, active_id));
                }
                DictationSlot::AwaitingInsertion(active_id) => {
                    *slot = DictationSlot::AwaitingInsertion(active_id);
                    return Err(stale_error(session_id, active_id));
                }
                DictationSlot::Finalizing(active_id) => {
                    *slot = DictationSlot::Finalizing(active_id);
                    return Err(busy_error("finalizing"));
                }
                DictationSlot::Inserting(active_id) => {
                    *slot = DictationSlot::Inserting(active_id);
                    return Err(busy_error("inserting"));
                }
                DictationSlot::Preparing => {
                    *slot = DictationSlot::Preparing;
                    return Err(busy_error("preparing"));
                }
                DictationSlot::Idle => {
                    return Err(DesktopDictationError::new(
                        DesktopDictationErrorKind::Busy,
                        "there is no active dictation to cancel",
                    ));
                }
            }
        };

        if let Some((key, recognizer)) = recognizer_to_recycle {
            self.recycle_recognizer(key, recognizer);
        }
        let result = self
            .capture
            .cancel_dictation(session_id)
            .map_err(map_capture_error);
        self.reset_to_idle();
        result
    }
'''
if old_cancel not in s:
    raise SystemExit("cancel block not found")
s = s.replace(old_cancel, new_cancel, 1)

needle = '''    fn reset_to_idle(&self) {
        *self.lock_slot() = DictationSlot::Idle;
    }

    fn lock_slot(&self) -> MutexGuard<'_, DictationSlot> {
'''
replacement = '''    fn acquire_recognizer(
        &self,
        model_path: &Path,
    ) -> Result<(RecognizerCacheKey, Box<dyn SpeechRecognizer>), RecognitionError> {
        let key = RecognizerCacheKey::for_path(model_path);
        if let Some(cached) = self.lock_recognizer_cache().take() {
            if cached.key == key {
                return Ok((key, cached.recognizer));
            }
        }
        self.recognizers.load(model_path).map(|recognizer| (key, recognizer))
    }

    fn recycle_recognizer(
        &self,
        key: RecognizerCacheKey,
        recognizer: Box<dyn SpeechRecognizer>,
    ) {
        *self.lock_recognizer_cache() = Some(CachedRecognizer { key, recognizer });
    }

    fn reset_to_idle(&self) {
        *self.lock_slot() = DictationSlot::Idle;
    }

    fn lock_recognizer_cache(&self) -> MutexGuard<'_, Option<CachedRecognizer>> {
        self.recognizer_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock_slot(&self) -> MutexGuard<'_, DictationSlot> {
'''
if needle not in s:
    raise SystemExit("service helper marker not found")
s = s.replace(needle, replacement, 1)

s = s.replace(
    '''    use blcvoice_audio::{
        AudioFailure, AudioSampleFormat, AudioStreamConfig, CaptureStats, InputCaptureFactory,
        InputCaptureSession, InputDeviceDiscovery, InputDiscovery,
    };
''',
    '''    use blcvoice_audio::{
        AudioFailure, AudioSampleFormat, AudioStreamConfig, CaptureStats, InputCaptureFactory,
        InputCaptureSession, InputDeviceDiscovery, InputDiscovery,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
''',
    1,
)

needle = '''    #[derive(Debug)]
    struct FailingRecognizerFactory;
'''
addition = '''    #[derive(Debug)]
    struct CountingRecognizerFactory {
        loads: Arc<AtomicUsize>,
    }

    impl RecognizerFactory for CountingRecognizerFactory {
        fn load(&self, _model_path: &Path) -> Result<Box<dyn SpeechRecognizer>, RecognitionError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(FakeRecognizer::new()))
        }
    }

''' + needle
if needle not in s:
    raise SystemExit("test factory marker not found")
s = s.replace(needle, addition, 1)

insert_before = '''    #[test]
    fn stale_finish_cannot_take_active_dictation() {
'''
cache_tests = '''    #[test]
    fn recognizer_is_reused_after_successful_dictation() {
        let loads = Arc::new(AtomicUsize::new(0));
        let service = service(Arc::new(CountingRecognizerFactory {
            loads: Arc::clone(&loads),
        }));

        let first = service.start(request()).expect("first dictation must start");
        service.finish(first.id).expect("first dictation must transcribe");
        service.begin_insertion(first.id).expect("insertion must begin");
        service.complete_insertion(first.id).expect("insertion must complete");

        let second = service.start(request()).expect("second dictation must start");
        assert_eq!(loads.load(Ordering::SeqCst), 1);
        service.cancel(second.id).expect("second dictation must cancel");
    }

    #[test]
    fn cancelled_recording_returns_recognizer_to_cache() {
        let loads = Arc::new(AtomicUsize::new(0));
        let service = service(Arc::new(CountingRecognizerFactory {
            loads: Arc::clone(&loads),
        }));

        let first = service.start(request()).expect("first dictation must start");
        service.cancel(first.id).expect("first dictation must cancel");
        let second = service.start(request()).expect("second dictation must start");
        assert_eq!(loads.load(Ordering::SeqCst), 1);
        service.cancel(second.id).expect("second dictation must cancel");
    }

    #[test]
    fn changing_model_path_replaces_single_entry_cache() {
        let loads = Arc::new(AtomicUsize::new(0));
        let service = service(Arc::new(CountingRecognizerFactory {
            loads: Arc::clone(&loads),
        }));

        let first = service.start(request()).expect("first dictation must start");
        service.cancel(first.id).expect("first dictation must cancel");
        let mut other = request();
        other.model_path = PathBuf::from("other-model.bin");
        let second = service.start(other).expect("other model must start");
        assert_eq!(loads.load(Ordering::SeqCst), 2);
        service.cancel(second.id).expect("second dictation must cancel");
    }

''' + insert_before
if insert_before not in s:
    raise SystemExit("test insertion marker not found")
s = s.replace(insert_before, cache_tests, 1)

path.write_text(s)
