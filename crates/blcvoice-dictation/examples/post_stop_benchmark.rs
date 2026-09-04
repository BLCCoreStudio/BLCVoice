use std::env;
use std::process;
use std::time::{Duration, Instant};

use blcvoice_asr::{
    AudioFormat, AudioInput, RecognitionError, RecognitionOptions, RecognizerCapabilities,
    SpeechRecognizer, TimestampGranularity, Transcription,
};
use blcvoice_audio::{
    AudioFailure, AudioSampleFormat, AudioStreamConfig, CaptureStats, InputCaptureSession,
};
use blcvoice_dictation::RecordingCollector;

const FORMAT_VERSION: &str = "blcvoice-benchmark-v1";
const SOURCE_SAMPLE_RATE_HZ: u32 = 48_000;
const SOURCE_CHANNELS: u16 = 2;
const ASR_SAMPLE_RATE_HZ: u32 = 16_000;
const DEFAULT_DURATION_SECONDS: u32 = 2;
const DEFAULT_RUNS: u32 = 5;

#[derive(Debug)]
struct Config {
    duration_seconds: u32,
    runs: u32,
}

#[derive(Debug)]
struct DeterministicCapture {
    config: AudioStreamConfig,
    samples: Vec<f32>,
    position: usize,
    stats: CaptureStats,
}

#[derive(Debug)]
struct MemoryEvidence {
    source: &'static str,
    current_semantics: &'static str,
    peak_semantics: &'static str,
    current_bytes: Option<u64>,
    peak_bytes: Option<u64>,
}

impl DeterministicCapture {
    fn new(duration_seconds: u32) -> Self {
        let samples = deterministic_stereo_input(duration_seconds);
        let received_samples = u64::try_from(samples.len()).unwrap_or(u64::MAX);
        Self {
            config: AudioStreamConfig {
                channels: SOURCE_CHANNELS,
                sample_rate_hz: SOURCE_SAMPLE_RATE_HZ,
                sample_format: AudioSampleFormat::F32,
            },
            samples,
            position: 0,
            stats: CaptureStats {
                received_samples,
                ..CaptureStats::default()
            },
        }
    }
}

impl InputCaptureSession for DeterministicCapture {
    fn stream_config(&self) -> &AudioStreamConfig {
        &self.config
    }

    fn read_interleaved_f32(&mut self, output: &mut [f32]) -> usize {
        let remaining = &self.samples[self.position..];
        let count = remaining.len().min(output.len());
        output[..count].copy_from_slice(&remaining[..count]);
        self.position += count;
        count
    }

    fn stats(&self) -> CaptureStats {
        self.stats
    }

    fn pause(&self) -> Result<(), AudioFailure> {
        Ok(())
    }

    fn resume(&self) -> Result<(), AudioFailure> {
        Ok(())
    }
}

#[derive(Debug)]
struct DeterministicRecognizer {
    capabilities: RecognizerCapabilities,
}

impl DeterministicRecognizer {
    fn new() -> Self {
        Self {
            capabilities: RecognizerCapabilities {
                required_audio_format: AudioFormat::new(1, ASR_SAMPLE_RATE_HZ)
                    .expect("constant ASR format is valid"),
                languages: Vec::new(),
                translation_targets: Vec::new(),
                max_timestamp_granularity: TimestampGranularity::None,
                supports_language_detection: false,
                supports_translation: false,
                supports_streaming: false,
                supports_cancellation: false,
                supports_punctuation_control: false,
                supports_inverse_text_normalization_control: false,
                max_audio_ms: None,
            },
        }
    }
}

impl SpeechRecognizer for DeterministicRecognizer {
    fn engine_id(&self) -> &'static str {
        "benchmark-deterministic"
    }

    fn model_id(&self) -> &str {
        "none"
    }

    fn backend_name(&self) -> &str {
        "synthetic"
    }

    fn capabilities(&self) -> &RecognizerCapabilities {
        &self.capabilities
    }

    fn transcribe(
        &mut self,
        input: AudioInput<'_>,
        _options: &RecognitionOptions,
    ) -> Result<Transcription, RecognitionError> {
        std::hint::black_box(input.samples());
        Ok(Transcription {
            text: "benchmark transcript".to_owned(),
            ..Transcription::default()
        })
    }
}

fn main() {
    let config = parse_args().unwrap_or_else(|message| {
        eprintln!("{message}");
        eprintln!("usage: post_stop_benchmark [--duration-seconds N] [--runs N]");
        process::exit(2);
    });

    let mut first_finalize = Duration::ZERO;
    let mut first_post_stop = Duration::ZERO;
    let mut warm_finalize_total = Duration::ZERO;
    let mut warm_post_stop_total = Duration::ZERO;
    let mut first_asr_frames = 0usize;
    let mut first_source_frames = 0usize;
    let mut transcript_bytes = 0usize;

    for run in 0..config.runs {
        let capture = DeterministicCapture::new(config.duration_seconds);
        let collector = RecordingCollector::new(Box::new(capture), config.duration_seconds * 1_000)
            .unwrap_or_else(|error| {
                eprintln!("collector setup failed: {error}");
                process::exit(1);
            });
        let mut recognizer = DeterministicRecognizer::new();
        let options = RecognitionOptions::default();

        let post_stop_started = Instant::now();
        let finalize_started = Instant::now();
        let finalized = collector.finalize().unwrap_or_else(|error| {
            eprintln!("finalization failed: {error}");
            process::exit(1);
        });
        let finalize_elapsed = finalize_started.elapsed();
        let capture = finalized
            .transcribe(&mut recognizer, &options)
            .unwrap_or_else(|error| {
                eprintln!("dictation transcription failed: {error}");
                process::exit(1);
            });
        let post_stop_elapsed = post_stop_started.elapsed();

        std::hint::black_box(&capture.transcription.text);
        transcript_bytes = transcript_bytes.saturating_add(capture.transcription.text.len());

        if run == 0 {
            first_finalize = finalize_elapsed;
            first_post_stop = post_stop_elapsed;
            first_source_frames = capture.source_frames;
            first_asr_frames = capture.asr_frames;
        } else {
            warm_finalize_total += finalize_elapsed;
            warm_post_stop_total += post_stop_elapsed;
        }
    }

    let warm_runs = config.runs.saturating_sub(1);
    let memory = memory_evidence();

    println!("format={FORMAT_VERSION}");
    println!("benchmark=dictation-post-stop-core");
    println!("recognizer=deterministic-contract-stub");
    println!("os={}", env::consts::OS);
    println!("arch={}", env::consts::ARCH);
    println!(
        "logical_cpus={}",
        std::thread::available_parallelism().map_or(0, usize::from)
    );
    println!("input_source=deterministic-generated");
    println!("source_sample_rate_hz={SOURCE_SAMPLE_RATE_HZ}");
    println!("source_channels={SOURCE_CHANNELS}");
    println!("asr_sample_rate_hz={ASR_SAMPLE_RATE_HZ}");
    println!("asr_channels=1");
    println!("input_duration_ms={}", config.duration_seconds * 1_000);
    println!("runs={}", config.runs);
    println!("warm_runs={warm_runs}");
    println!("source_frames={first_source_frames}");
    println!("asr_frames={first_asr_frames}");
    println!("first_finalize_ms={:.3}", millis(first_finalize));
    println!(
        "first_post_stop_to_transcript_ms={:.3}",
        millis(first_post_stop)
    );
    if warm_runs > 0 {
        println!(
            "warm_finalize_mean_ms={:.3}",
            millis(warm_finalize_total / warm_runs)
        );
        println!(
            "warm_post_stop_to_transcript_mean_ms={:.3}",
            millis(warm_post_stop_total / warm_runs)
        );
    }
    println!("transcript_bytes={transcript_bytes}");
    println!("memory_source={}", memory.source);
    println!("memory_current_semantics={}", memory.current_semantics);
    println!("memory_peak_semantics={}", memory.peak_semantics);
    print_optional_u64("memory_current_bytes", memory.current_bytes);
    print_optional_u64("memory_peak_bytes", memory.peak_bytes);
}

fn parse_args() -> Result<Config, String> {
    let mut args = env::args().skip(1);
    let mut duration_seconds = DEFAULT_DURATION_SECONDS;
    let mut runs = DEFAULT_RUNS;

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--duration-seconds" => {
                duration_seconds =
                    parse_positive(&next_value(&mut args, "--duration-seconds")?, "duration")?;
            }
            "--runs" => {
                runs = parse_positive(&next_value(&mut args, "--runs")?, "runs")?;
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    Ok(Config {
        duration_seconds,
        runs,
    })
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_positive(value: &str, name: &str) -> Result<u32, String> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| format!("{name} must be a positive integer"))?;
    if parsed == 0 {
        return Err(format!("{name} must be greater than zero"));
    }
    Ok(parsed)
}

fn deterministic_stereo_input(duration_seconds: u32) -> Vec<f32> {
    let frames = usize::try_from(SOURCE_SAMPLE_RATE_HZ)
        .expect("sample rate fits usize")
        .saturating_mul(usize::try_from(duration_seconds).expect("duration fits usize"));
    let mut samples = Vec::with_capacity(frames.saturating_mul(usize::from(SOURCE_CHANNELS)));

    for index in 0..frames {
        let time = index as f32 / SOURCE_SAMPLE_RATE_HZ as f32;
        let base = (time * 2.0 * std::f32::consts::PI * 220.0).sin() * 0.04;
        samples.push(base);
        samples.push(base * 0.8);
    }

    samples
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn print_optional_u64(key: &str, value: Option<u64>) {
    match value {
        Some(value) => println!("{key}={value}"),
        None => println!("{key}=unavailable"),
    }
}

#[cfg(target_os = "linux")]
fn memory_evidence() -> MemoryEvidence {
    let status = std::fs::read_to_string("/proc/self/status").ok();
    let current_bytes = status
        .as_deref()
        .and_then(|value| parse_linux_status_kib(value, "VmRSS:"));
    let peak_bytes = status
        .as_deref()
        .and_then(|value| parse_linux_status_kib(value, "VmHWM:"));

    MemoryEvidence {
        source: "proc-self-status",
        current_semantics: "linux-vmrss-resident-set",
        peak_semantics: "linux-vmhwm-resident-set-high-water-mark",
        current_bytes,
        peak_bytes,
    }
}

#[cfg(target_os = "linux")]
fn parse_linux_status_kib(status: &str, key: &str) -> Option<u64> {
    let kib = status.lines().find_map(|line| {
        let rest = line.strip_prefix(key)?;
        rest.split_whitespace().next()?.parse::<u64>().ok()
    })?;
    kib.checked_mul(1024)
}

#[cfg(target_os = "windows")]
fn memory_evidence() -> MemoryEvidence {
    let pid = process::id();
    let command = format!(
        "$p=Get-Process -Id {pid}; Write-Output ($p.WorkingSet64.ToString() + ',' + $p.PeakWorkingSet64.ToString())"
    );
    let output = process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &command])
        .output()
        .ok();
    let parsed = output.as_ref().and_then(|result| {
        if !result.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&result.stdout);
        let mut values = text.trim().split(',');
        let current = values.next()?.trim().parse::<u64>().ok()?;
        let peak = values.next()?.trim().parse::<u64>().ok()?;
        Some((current, peak))
    });

    MemoryEvidence {
        source: "windows-get-process",
        current_semantics: "windows-working-set-size",
        peak_semantics: "windows-peak-working-set-size",
        current_bytes: parsed.map(|value| value.0),
        peak_bytes: parsed.map(|value| value.1),
    }
}

#[cfg(target_os = "macos")]
fn memory_evidence() -> MemoryEvidence {
    let pid = process::id().to_string();
    let output = process::Command::new("/bin/ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok();
    let current_bytes = output.as_ref().and_then(|result| {
        if !result.status.success() {
            return None;
        }
        let kib = String::from_utf8_lossy(&result.stdout)
            .trim()
            .parse::<u64>()
            .ok()?;
        kib.checked_mul(1024)
    });

    MemoryEvidence {
        source: "macos-ps-rss",
        current_semantics: "macos-ps-resident-set",
        peak_semantics: "external-usr-bin-time-l-required",
        current_bytes,
        peak_bytes: None,
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn memory_evidence() -> MemoryEvidence {
    MemoryEvidence {
        source: "unsupported-platform",
        current_semantics: "unavailable",
        peak_semantics: "unavailable",
        current_bytes: None,
        peak_bytes: None,
    }
}
