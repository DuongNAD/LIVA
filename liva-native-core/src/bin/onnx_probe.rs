//! Diagnostic: print the input/output tensor contract of any ONNX model.
//!
//! Usage: onnx_probe <model.onnx>

use ort::session::Session;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: onnx_probe <model.onnx>");
        std::process::exit(1);
    }

    let session = match Session::builder().and_then(|b| b.commit_from_file(&args[1])) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to load model: {}", e);
            std::process::exit(1);
        }
    };

    println!("=== INPUTS ===");
    for input in session.inputs() {
        println!("  {:#?}", input);
    }
    println!("=== OUTPUTS ===");
    for output in session.outputs() {
        println!("  {:#?}", output);
    }
}
