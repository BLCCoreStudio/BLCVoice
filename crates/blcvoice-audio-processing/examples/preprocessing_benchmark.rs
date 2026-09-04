#![forbid(unsafe_code)]

use std::env;
use std::error::Error;
use std::time::{Duration, Instant};

use blcvoice_audio_processing::{AudioFormat, AudioPreprocessor};

const SOURCE_RATE_HZ: u32 = 48_000;
const SOURCE_CHANNELS: u16 = 2;
const TARGET_RATE_HZ: u32 = 16_000;
const TARGET_CHANNELS: u16 = 1;
const DEFAULT_DURATION_SECONDS: u32 = 5;
const DEFAULT_WARM_RUNS: usize = 20;

#[derive(Debug, Clone, Copy)]
struct Config {
    duration_seconds: u32,
    warm_runs: usize,
}

impl Config {
    fn from_args() -> Result<Self, Box<dyn Error>> {
        let mut duration_seconds = DEFAULT_DURATION_SECONDS;
        let mut warm_runs = DEFAULT_WARM_RUNS;
        let mut args = env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--duration-seconds" => {
                    duration_seconds = args
                        .next()
                        .ok_or("--duration-seconds requires a value")?
                        .parse()?;
                    if duration_seconds == 0 {
                        return Err("--duration-seconds must be greater than zero".into());
                    }
                }
                "--warm-runs" => {
                    warm_runs = args
                        .next()
                        .ok_or("--warm-runs requires a value")?
                        .parse()?;
                    if warm_runs == 0 {
                        return Err("--warm-runs must be greater than zero".into());
                    }
                }
                "--help" | "-h" => {
                    println!(
                        "Usage: preprocessing_benchmark [--duration-seconds N] [--warm-runs N]"
                    );
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument: {other}").into()),
            }
        }

        Ok(Self {
            duration_seconds,
            warm_runs,
        })
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::from_args()?;
    let source = AudioFormat::new(SOURCE_CHANNELS, SOURCE_RATE_HZ)?;
    let target = AudioFormat::new(TARGET_CHANNELS, TARGET_RATE_HZ)?;
    let input = deterministic_stereo_input(config.duration_seconds)?;
    let audio_duration = Duration::from_secs(u64::from(config.duration_seconds));

    let construction_started = Instant::now();
    let mut processor = AudioPreprocessor::new(source, target)?;
    let construction_elapsed = construction_started.elapsed();

    let cold_started = Instant::now();
    let cold_frames = {
        let output = processor.process_utterance(&input)?;
        output.frames()
    };
    let cold_elapsed = cold_started.elapsed();

    let mut warm_samples = Vec::with_capacity(config.warm_runs);
    for _ in 0..config.warm_runs {
        let started = Instant::now();
        let frames = {
            let output = processor.process_utterance(&input)?;
            output.frames()
        };
        if frames != cold_frames {
            return Err("preprocessor produced inconsistent frame counts".into());
        }
        warm_samples.push(started.elapsed());
    }
    warm_samples.sort_unstable();

    let warm_median = percentile(&warm_samples, 50);
    let warm_p95 = percentile(&warm_samples, 95);

    println!("format=blcvoice-benchmark-v1");
    println!("benchmark=audio_preprocessing_48k_stereo_to_16k_mono");
    println!(
        "git_commit={}",
        option_env!("GIT_COMMIT").unwrap_or("unknown")
    );
    println!("os={}", env::consts::OS);
    println!("arch={}", env::consts::ARCH);
    println!(
        "logical_cpus={}",
        std::thread::available_parallelism().map_or(0, usize::from)
    );
    println!("source_rate_hz={SOURCE_RATE_HZ}");
    println!("source_channels={SOURCE_CHANNELS}");
    println!("target_rate_hz={TARGET_RATE_HZ}");
    println!("target_channels={TARGET_CHANNELS}");
    println!("audio_duration_ms={}", audio_duration.as_millis());
    println!("input_samples={}", input.len());
    println!("output_frames={cold_frames}");
    println!("warm_runs={}", config.warm_runs);
    print_duration("construction", construction_elapsed, audio_duration);
    print_duration("cold", cold_elapsed, audio_duration);
    print_duration("warm_median", warm_median, audio_duration);
    print_duration("warm_p95", warm_p95, audio_duration);

    Ok(())
}

fn deterministic_stereo_input(duration_seconds: u32) -> Result<Vec<f32>, Box<dyn Error>> {
    let frames = usize::try_from(
        u64::from(SOURCE_RATE_HZ)
            .checked_mul(u64::from(duration_seconds))
            .ok_or("input frame count overflow")?,
    )?;
    let sample_count = frames
        .checked_mul(usize::from(SOURCE_CHANNELS))
        .ok_or("input sample count overflow")?;

    let mut samples = Vec::with_capacity(sample_count);
    for frame in 0..frames {
        let phase = (frame % 257) as f32 / 256.0;
        let sample = (phase * 2.0) - 1.0;
        samples.push(sample * 0.25);
        samples.push(sample * -0.25);
    }
    Ok(samples)
}

fn percentile(samples: &[Duration], percentile: usize) -> Duration {
    debug_assert!(!samples.is_empty());
    debug_assert!((1..=100).contains(&percentile));

    let rank = samples.len().saturating_mul(percentile).div_ceil(100);
    samples[rank.saturating_sub(1).min(samples.len() - 1)]
}

fn print_duration(label: &str, elapsed: Duration, audio_duration: Duration) {
    let realtime_factor = elapsed.as_secs_f64() / audio_duration.as_secs_f64();
    println!("{label}_elapsed_us={}", elapsed.as_micros());
    println!("{label}_rtf={realtime_factor:.6}");
}
