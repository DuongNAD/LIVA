use liva_rustpotter_probe::{PersonalWakeConfig, PersonalWakeDetector, benchmark, wav_files};
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("rustpotter-probe: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let enrollment_dir = required_path(&mut args, "<enrollment-dir>")?;
    let positive_dir = required_path(&mut args, "<positive-dir>")?;
    let negative_dir = required_path(&mut args, "<negative-dir>")?;
    let limit = optional(&mut args, "limit")?;
    let enrollment_limit = optional(&mut args, "enrollment-limit")?.unwrap_or(5);
    let threshold = optional_f32(&mut args, "threshold")?;
    let avg_threshold = optional_f32(&mut args, "avg-threshold")?;
    let min_scores = optional(&mut args, "min-scores")?;
    if args.next().is_some() {
        return Err(usage());
    }

    let enrollment = wav_files(&enrollment_dir, Some(enrollment_limit))?;
    let positives = wav_files(&positive_dir, limit)?;
    let negatives = wav_files(&negative_dir, limit)?;
    let mut config = PersonalWakeConfig::default();
    if let Some(value) = threshold {
        config.threshold = value;
    }
    if let Some(value) = avg_threshold {
        config.avg_threshold = value;
    }
    if let Some(value) = min_scores {
        config.min_scores = value;
    }
    let mut detector = PersonalWakeDetector::enroll(&enrollment, &config)?;
    let metrics = benchmark(&mut detector, &enrollment, &positives, &negatives)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&metrics)
            .map_err(|error| format!("cannot serialize metrics: {error}"))?
    );
    Ok(())
}

fn required_path(args: &mut impl Iterator<Item = String>, label: &str) -> Result<PathBuf, String> {
    args.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing {label}\n{}", usage()))
}

fn optional(args: &mut impl Iterator<Item = String>, label: &str) -> Result<Option<usize>, String> {
    args.next()
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| format!("invalid {label}: {value}"))
        })
        .transpose()
}

fn optional_f32(
    args: &mut impl Iterator<Item = String>,
    label: &str,
) -> Result<Option<f32>, String> {
    args.next()
        .map(|value| {
            value
                .parse::<f32>()
                .map_err(|_| format!("invalid {label}: {value}"))
        })
        .transpose()
}

fn usage() -> String {
    "usage: liva-rustpotter-probe <enrollment-dir> <positive-dir> <negative-dir> [limit] [enrollment-limit] [threshold] [avg-threshold] [min-scores]".to_owned()
}
