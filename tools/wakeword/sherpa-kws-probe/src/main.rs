use liva_sherpa_kws_probe::{SherpaWakeConfig, SherpaWakeDetector, benchmark_corpus, wav_files};
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("sherpa-kws-probe: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let model_dir = required_path(&mut args, "<model-dir>")?;
    let positive_dir = required_path(&mut args, "<positive-dir>")?;
    let negative_dir = required_path(&mut args, "<negative-dir>")?;
    let limit = args
        .next()
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| format!("invalid limit: {value}"))
        })
        .transpose()?;
    let threshold = optional_f32(&mut args, "threshold")?;
    let score = optional_f32(&mut args, "score")?;
    if args.next().is_some() {
        return Err(usage());
    }

    let mut config = SherpaWakeConfig::for_model_dir(model_dir);
    if let Some(threshold) = threshold {
        config.keywords_threshold = threshold;
    }
    if let Some(score) = score {
        config.keywords_score = score;
    }
    let detector = SherpaWakeDetector::new(config)?;
    let positives = wav_files(&positive_dir, limit)?;
    let negatives = wav_files(&negative_dir, limit)?;
    if positives.is_empty() || negatives.is_empty() {
        return Err("positive and negative corpora must both contain WAV files".to_owned());
    }
    let metrics = benchmark_corpus(&detector, &positives, &negatives)?;
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
    "usage: liva-sherpa-kws-probe <model-dir> <positive-dir> <negative-dir> [limit] [threshold] [score]".to_owned()
}
