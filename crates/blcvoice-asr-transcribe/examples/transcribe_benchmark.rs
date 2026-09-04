use std::env;
use std::path::PathBuf;
use std::process;
use std::time::{Duration, Instant};

use blcvoice_asr::{AudioFormat, AudioInput, RecognitionOptions, SpeechRecognizer};
use blcvoice_asr_transcribe::{
    TranscribeBackend, TranscribeRecognizer, TranscribeRecognizerConfig,
};

const FORMAT_VERSION: &str = "blcvoice-benchmark-v1";
const SAMPLE_RATE_HZ: u32 = 16_000;
const CHANNELS: u16 = 1;

#[derive(Debug)]
struct Config {
    model: PathBuf,
    duration_seconds: u32,
    warm_runs: u32,
    n_threads: i32,
}

fn main() {
    let config = parse_args().unwrap_or_else(|message| {
        eprintln!("{message}");
        eprintln!(
            "usage: transcribe_benchmark --model PATH [--duration-seconds N] [--warm-runs N] [--threads N]"
        );
        process::exit(2);
    });

    let samples = deterministic_input(config.duration_seconds);
    let format =
        AudioFormat::new(CHANNELS, SAMPLE_RATE_HZ).expect("constant audio format is valid");
    let input = AudioInput::new(&samples, format).expect("generated benchmark audio is valid");
    let audio_duration = Duration::from_secs(u64::from(config.duration_seconds));

    let load_started = Instant::now();
    let mut recognizer = TranscribeRecognizer::load(
        &config.model,
        TranscribeRecognizerConfig {
            backend: TranscribeBackend::Auto,
            n_threads: config.n_threads,
        },
    )
    .unwrap_or_else(|error| {
        eprintln!("model load failed: {error}");
        process::exit(1);
    });
    let load_elapsed = load_started.elapsed();

    let options = RecognitionOptions::default();
    let cold_started = Instant::now();
    let cold = recognizer
        .transcribe(input, &options)
        .unwrap_or_else(|error| {
            eprintln!("cold inference failed: {error}");
            process::exit(1);
        });
    let cold_elapsed = cold_started.elapsed();

    let mut warm_total = Duration::ZERO;
    let mut warm_text_bytes = 0usize;
    for _ in 0..config.warm_runs {
        let started = Instant::now();
        let transcript = recognizer
            .transcribe(input, &options)
            .unwrap_or_else(|error| {
                eprintln!("warm inference failed: {error}");
                process::exit(1);
            });
        warm_total += started.elapsed();
        warm_text_bytes = warm_text_bytes.saturating_add(transcript.text.len());
    }

    let warm_mean = warm_total / config.warm_runs;

    println!("format={FORMAT_VERSION}");
    println!("benchmark=asr-transcribe");
    println!("engine_id={}", recognizer.engine_id());
    println!("adapter_version={}", env!("CARGO_PKG_VERSION"));
    println!("model_id={}", recognizer.model_id());
    println!("model_path={}", config.model.display());
    println!("backend={}", recognizer.backend_name());
    println!("threads={}", config.n_threads);
    println!("os={}", env::consts::OS);
    println!("arch={}", env::consts::ARCH);
    println!(
        "logical_cpus={}",
        std::thread::available_parallelism().map_or(0, usize::from)
    );
    println!("input_source=deterministic-generated");
    println!("input_sample_rate_hz={SAMPLE_RATE_HZ}");
    println!("input_channels={CHANNELS}");
    println!("input_duration_ms={}", audio_duration.as_millis());
    println!("input_samples={}", samples.len());
    println!("warm_runs={}", config.warm_runs);
    println!("model_load_ms={:.3}", millis(load_elapsed));
    println!("cold_inference_ms={:.3}", millis(cold_elapsed));
    println!(
        "cold_inference_rtf={:.6}",
        rtf(cold_elapsed, audio_duration)
    );
    println!("warm_inference_total_ms={:.3}", millis(warm_total));
    println!("warm_inference_mean_ms={:.3}", millis(warm_mean));
    println!(
        "warm_inference_mean_rtf={:.6}",
        rtf(warm_mean, audio_duration)
    );
    println!("cold_transcript_bytes={}", cold.text.len());
    println!("warm_transcript_bytes={warm_text_bytes}");
}

fn parse_args() -> Result<Config, String> {
    let mut args = env::args().skip(1);
    let mut model = None;
    let mut duration_seconds = 2u32;
    let mut warm_runs = 3u32;
    let mut n_threads = 0i32;

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--model" => model = Some(PathBuf::from(next_value(&mut args, "--model")?)),
            "--duration-seconds" => {
                duration_seconds =
                    parse_positive(&next_value(&mut args, "--duration-seconds")?, "duration")?;
            }
            "--warm-runs" => {
                warm_runs = parse_positive(&next_value(&mut args, "--warm-runs")?, "warm runs")?;
            }
            "--threads" => {
                n_threads = next_value(&mut args, "--threads")?
                    .parse::<i32>()
                    .map_err(|_| "threads must be a non-negative integer".to_owned())?;
                if n_threads < 0 {
                    return Err("threads must be a non-negative integer".to_owned());
                }
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    Ok(Config {
        model: model.ok_or_else(|| "--model PATH is required".to_owned())?,
        duration_seconds,
        warm_runs,
        n_threads,
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

fn deterministic_input(duration_seconds: u32) -> Vec<f32> {
    let sample_count = usize::try_from(SAMPLE_RATE_HZ)
        .expect("sample rate fits usize")
        .saturating_mul(usize::try_from(duration_seconds).expect("duration fits usize"));

    (0..sample_count)
        .map(|index| {
            let time = index as f32 / SAMPLE_RATE_HZ as f32;
            let carrier = (time * 2.0 * std::f32::consts::PI * 220.0).sin();
            let modulator = (time * 2.0 * std::f32::consts::PI * 3.0).sin();
            carrier * (0.04 + 0.01 * modulator)
        })
        .collect()
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn rtf(elapsed: Duration, audio_duration: Duration) -> f64 {
    elapsed.as_secs_f64() / audio_duration.as_secs_f64()
}
