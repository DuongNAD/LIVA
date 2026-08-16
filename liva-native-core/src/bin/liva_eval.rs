//! `liva_eval` — CLI Automated Evaluation & Benchmark Harness (Features F10, F11).
//!
//! Inspired by DeepSeek Harness / Cordis benchmark runners.
//! Measures Raw TTFT, Visible TTFT, TPS, Tool Call Accuracy, Argument Validation,
//! and CoT Soundness across Local GGUF models and Cloud APIs.
//!
//! Usage:
//! ```powershell
//! # Run default benchmark suite with mock provider:
//! cargo run --bin liva_eval
//!
//! # Run custom suite with cloud API:
//! cargo run --bin liva_eval -- --suite benchmarks/core.json --provider cloud --api-base https://api.deepseek.com/v1 --api-key $env:DEEPSEEK_API_KEY --model deepseek-chat
//!
//! # CI Gate check:
//! cargo run --bin liva_eval -- --min-accuracy 0.90 --max-ttft-p95 850 --out json
//!
//! # Compare two reports:
//! cargo run --bin liva_eval -- --compare report_baseline.json report_challenger.json
//! ```

use liva_native_core::eval::{
    BenchmarkReport, BenchmarkRunner, BenchmarkSuite, CloudApiProvider, ComparativeMatrix,
    EvaluationConfig, EvaluationProvider, MockProvider,
};
use std::path::PathBuf;
use std::sync::Arc;

struct CliArgs {
    suite_path: Option<PathBuf>,
    model_name: Option<String>,
    provider_type: String,
    api_base: String,
    api_key: String,
    warmup_runs: usize,
    runs_per_case: usize,
    out_format: String,
    min_accuracy: Option<f64>,
    max_ttft_p95: Option<u64>,
    output_file: Option<PathBuf>,
    compare_files: Option<(PathBuf, PathBuf)>,
    show_help: bool,
}

fn parse_args() -> Result<CliArgs, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut suite_path = None;
    let mut model_name = None;
    let mut provider_type = "mock".to_string();
    let mut api_base = std::env::var("LIVA_EVAL_API_BASE")
        .or_else(|_| std::env::var("OPENAI_BASE_URL"))
        .unwrap_or_else(|_| "http://127.0.0.1:8000/v1".to_string());
    let mut api_key = std::env::var("LIVA_EVAL_API_KEY")
        .or_else(|_| std::env::var("DEEPSEEK_API_KEY"))
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .unwrap_or_default();
    let mut warmup_runs = 1;
    let mut runs_per_case = 3;
    let mut out_format = "md".to_string();
    let mut min_accuracy = None;
    let mut max_ttft_p95 = None;
    let mut output_file = None;
    let mut compare_files = None;
    let mut show_help = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                show_help = true;
                i += 1;
            }
            "--suite" => {
                i += 1;
                if i >= args.len() {
                    return Err("Missing value for --suite".into());
                }
                suite_path = Some(PathBuf::from(&args[i]));
                i += 1;
            }
            "--model" => {
                i += 1;
                if i >= args.len() {
                    return Err("Missing value for --model".into());
                }
                model_name = Some(args[i].clone());
                i += 1;
            }
            "--provider" => {
                i += 1;
                if i >= args.len() {
                    return Err("Missing value for --provider".into());
                }
                provider_type = args[i].to_lowercase();
                i += 1;
            }
            "--api-base" => {
                i += 1;
                if i >= args.len() {
                    return Err("Missing value for --api-base".into());
                }
                api_base = args[i].clone();
                i += 1;
            }
            "--api-key" => {
                i += 1;
                if i >= args.len() {
                    return Err("Missing value for --api-key".into());
                }
                api_key = args[i].clone();
                i += 1;
            }
            "--warmup" => {
                i += 1;
                if i >= args.len() {
                    return Err("Missing value for --warmup".into());
                }
                warmup_runs = args[i]
                    .parse()
                    .map_err(|_| format!("Invalid warmup number: {}", args[i]))?;
                i += 1;
            }
            "--runs" => {
                i += 1;
                if i >= args.len() {
                    return Err("Missing value for --runs".into());
                }
                runs_per_case = args[i]
                    .parse()
                    .map_err(|_| format!("Invalid runs number: {}", args[i]))?;
                i += 1;
            }
            "--out" => {
                i += 1;
                if i >= args.len() {
                    return Err("Missing value for --out".into());
                }
                out_format = args[i].to_lowercase();
                i += 1;
            }
            "--min-accuracy" => {
                i += 1;
                if i >= args.len() {
                    return Err("Missing value for --min-accuracy".into());
                }
                min_accuracy = Some(
                    args[i]
                        .parse()
                        .map_err(|_| format!("Invalid min-accuracy value: {}", args[i]))?,
                );
                i += 1;
            }
            "--max-ttft-p95" => {
                i += 1;
                if i >= args.len() {
                    return Err("Missing value for --max-ttft-p95".into());
                }
                max_ttft_p95 = Some(
                    args[i]
                        .parse()
                        .map_err(|_| format!("Invalid max-ttft-p95 value: {}", args[i]))?,
                );
                i += 1;
            }
            "--output-file" => {
                i += 1;
                if i >= args.len() {
                    return Err("Missing value for --output-file".into());
                }
                output_file = Some(PathBuf::from(&args[i]));
                i += 1;
            }
            "--compare" => {
                i += 1;
                if i + 1 >= args.len() {
                    return Err("--compare requires two file paths: <file1> <file2>".into());
                }
                let f1 = PathBuf::from(&args[i]);
                let f2 = PathBuf::from(&args[i + 1]);
                compare_files = Some((f1, f2));
                i += 2;
            }
            unknown => return Err(format!("Unknown argument: {unknown}")),
        }
    }

    Ok(CliArgs {
        suite_path,
        model_name,
        provider_type,
        api_base,
        api_key,
        warmup_runs,
        runs_per_case,
        out_format,
        min_accuracy,
        max_ttft_p95,
        output_file,
        compare_files,
        show_help,
    })
}

fn print_help() {
    println!(
        r#"LIVA-Eval: Automated Evaluation & Benchmark Harness

Usage:
  liva_eval [options]

Options:
  --suite <path>            Path to benchmark suite JSON file (default: built-in suite)
  --model <name|path>       Model identifier or GGUF path (default: "default_model")
  --provider <type>         Provider: 'mock', 'cloud', 'local' (default: 'mock')
  --api-base <url>          Base URL for OpenAI/DeepSeek compatible endpoint
  --api-key <key>           API key for cloud endpoint (reads DEEPSEEK_API_KEY/OPENAI_API_KEY)
  --warmup <n>              Number of warmup runs per case (default: 1)
  --runs <n>                Number of measured evaluation runs per case (default: 3)
  --out <json|md|summary>   Output format (default: 'md')
  --min-accuracy <float>    CI/CD Gate: minimum required accuracy (e.g. 0.90)
  --max-ttft-p95 <int>      CI/CD Gate: maximum allowed visible TTFT p95 in ms (e.g. 850)
  --output-file <path>      Save output report to file path
  --compare <f1> <f2>       Compare two benchmark JSON reports side-by-side
  --help, -h                Show this help message
"#
    );
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("Error: {e}\nRun with --help for usage instructions.");
            std::process::exit(1);
        }
    };

    if args.show_help {
        print_help();
        return Ok(());
    }

    // Handle comparison mode
    if let Some((f1, f2)) = args.compare_files {
        let content1 = std::fs::read_to_string(&f1)
            .map_err(|e| format!("Failed to read report 1 ({:?}): {e}", f1))?;
        let content2 = std::fs::read_to_string(&f2)
            .map_err(|e| format!("Failed to read report 2 ({:?}): {e}", f2))?;

        let report1: BenchmarkReport = serde_json::from_str(&content1)
            .map_err(|e| format!("Invalid JSON in report 1: {e}"))?;
        let report2: BenchmarkReport = serde_json::from_str(&content2)
            .map_err(|e| format!("Invalid JSON in report 2: {e}"))?;

        let matrix = ComparativeMatrix::new(report1, report2);
        println!("{}", matrix.to_markdown());
        return Ok(());
    }

    // Load benchmark suite
    let suite = match args.suite_path {
        Some(path) => {
            println!("Loading benchmark suite from: {:?}", path);
            BenchmarkSuite::from_file(&path)?
        }
        None => {
            println!("Using built-in authoritative LIVA benchmark suite.");
            BenchmarkSuite::default_suite()
        }
    };

    let model_str = args
        .model_name
        .unwrap_or_else(|| "gemma-4-E4B-it-qat".to_string());

    // Create provider
    let provider: Arc<dyn EvaluationProvider> = match args.provider_type.as_str() {
        "cloud" => {
            println!(
                "Configuring Cloud API provider (endpoint: {}, model: {})...",
                args.api_base, model_str
            );
            Arc::new(CloudApiProvider::new(
                &args.api_base,
                &args.api_key,
                &model_str,
            ))
        }
        _ => {
            println!(
                "Configuring Evaluation provider '{}' (model: {})...",
                args.provider_type, model_str
            );
            Arc::new(MockProvider::new(&args.provider_type, &model_str))
        }
    };

    let config = EvaluationConfig {
        warmup_runs: args.warmup_runs,
        runs_per_case: args.runs_per_case,
        inject_nonce: true,
        min_accuracy_gate: args.min_accuracy,
        max_ttft_p95_gate_ms: args.max_ttft_p95,
    };

    println!(
        "Running benchmark suite '{}' ({} test cases, {} warmup + {} evaluated runs/case)...",
        suite.suite_name,
        suite.test_cases.len(),
        config.warmup_runs,
        config.runs_per_case
    );

    let runner = BenchmarkRunner::new(config.clone(), provider);
    let report = runner.run_suite(&suite).await?;

    // Render output
    let output_text = match args.out_format.as_str() {
        "json" => serde_json::to_string_pretty(&report)?,
        "summary" => format!(
            "Suite: {} | Accuracy: {:.1}% | TTFT p95: {} ms | TPS: {:.1}",
            report.suite_name,
            report.overall_accuracy * 100.0,
            report.visible_ttft.p95,
            report.mean_tps
        ),
        _ => report.to_markdown(),
    };

    println!("\n{}", output_text);

    if let Some(out_path) = args.output_file {
        std::fs::write(&out_path, &output_text)
            .map_err(|e| format!("Failed to write output report to {:?}: {e}", out_path))?;
        println!("Report saved to: {:?}", out_path);
    }

    // Check gating criteria
    if let Err(violations) = report.check_gates(args.min_accuracy, args.max_ttft_p95) {
        eprintln!("\n❌ CI Gate Check Failed:");
        for v in violations {
            eprintln!("  - {v}");
        }
        std::process::exit(1);
    }

    println!("\n✅ Evaluation completed successfully.");
    Ok(())
}
