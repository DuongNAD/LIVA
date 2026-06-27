mod g2p;
mod tokenizer;

use ort::{session::Session, value::Value};
use std::env;
use std::fs::File;
use std::io::Write;
use std::process::Command;
use std::path::PathBuf;

fn create_wav_header(num_samples: usize) -> [u8; 44] {
    let sample_rate = 24000u32;
    let num_channels = 1u16;
    let bits_per_sample = 16u16;
    
    let subchunk2_size = (num_samples * num_channels as usize * (bits_per_sample as usize / 8)) as u32;
    let chunk_size = 36 + subchunk2_size;
    let byte_rate = sample_rate * num_channels as u32 * (bits_per_sample as u32 / 8);
    let block_align = num_channels * (bits_per_sample / 8);

    let mut header = [0u8; 44];
    
    // ChunkID
    header[0..4].copy_from_slice(b"RIFF");
    // ChunkSize
    header[4..8].copy_from_slice(&chunk_size.to_le_bytes());
    // Format
    header[8..12].copy_from_slice(b"WAVE");
    // Subchunk1ID
    header[12..16].copy_from_slice(b"fmt ");
    // Subchunk1Size (16 for PCM)
    header[16..20].copy_from_slice(&16u32.to_le_bytes());
    // AudioFormat (1 for PCM)
    header[20..22].copy_from_slice(&1u16.to_le_bytes());
    // NumChannels
    header[22..24].copy_from_slice(&num_channels.to_le_bytes());
    // SampleRate
    header[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    // ByteRate
    header[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    // BlockAlign
    header[32..34].copy_from_slice(&block_align.to_le_bytes());
    // BitsPerSample
    header[34..36].copy_from_slice(&bits_per_sample.to_le_bytes());
    // Subchunk2ID
    header[36..40].copy_from_slice(b"data");
    // Subchunk2Size
    header[40..44].copy_from_slice(&subchunk2_size.to_le_bytes());

    header
}

fn resolve_poc_dir() -> PathBuf {
    if let Ok(val) = env::var("OMNIVOICE_POC_DIR") {
        return PathBuf::from(val);
    }

    if let Ok(cwd) = env::current_dir() {
        let mut dir = cwd.clone();
        loop {
            if dir.join("export_onnx.py").exists() {
                return dir;
            }
            if !dir.pop() {
                break;
            }
        }
    }

    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let mut dir = exe_dir.to_path_buf();
            loop {
                if dir.join("export_onnx.py").exists() {
                    return dir;
                }
                if !dir.pop() {
                    break;
                }
            }
        }
    }

    PathBuf::from(".")
}

fn find_python_interpreter() -> String {
    let interpreters = ["python", "python3", "py"];
    for py in interpreters {
        if let Ok(output) = Command::new(py).arg("--version").output() {
            if output.status.success() {
                return py.to_string();
            }
        }
    }
    "python".to_string()
}

fn split_text(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    let mut char_count = 0;

    for c in text.chars() {
        current_chunk.push(c);
        char_count += 1;
        if c == '.' || c == '!' || c == '?' {
            let trimmed = current_chunk.trim();
            if !trimmed.is_empty() {
                chunks.push(trimmed.to_string());
            }
            current_chunk.clear();
            char_count = 0;
        } else if char_count >= 400 {
            if let Some(last_space_idx) = current_chunk.rfind(' ') {
                let (left, right) = current_chunk.split_at(last_space_idx);
                let trimmed_left = left.trim();
                if !trimmed_left.is_empty() {
                    chunks.push(trimmed_left.to_string());
                }
                let remainder = right.trim_start();
                char_count = remainder.chars().count();
                current_chunk = remainder.to_string();
            } else {
                let trimmed = current_chunk.trim();
                if !trimmed.is_empty() {
                    chunks.push(trimmed.to_string());
                }
                current_chunk.clear();
                char_count = 0;
            }
        }
    }

    let trimmed = current_chunk.trim();
    if !trimmed.is_empty() {
        chunks.push(trimmed.to_string());
    }

    if chunks.is_empty() {
        chunks.push(" ".to_string());
    }

    chunks
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let mut text_opt = None;
    let mut reference_opt = None;
    let mut output_opt = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--text" => text_opt = args.next(),
            "--reference" => reference_opt = args.next(),
            "--output" => output_opt = args.next(),
            _ => {
                eprintln!("Unknown argument: {}", arg);
                std::process::exit(1);
            }
        }
    }

    let text = match text_opt {
        Some(t) => t,
        None => {
            eprintln!("Error: Missing required argument --text");
            std::process::exit(1);
        }
    };
    let reference = match reference_opt {
        Some(r) => r,
        None => {
            eprintln!("Error: Missing required argument --reference");
            std::process::exit(1);
        }
    };
    let output = match output_opt {
        Some(o) => o,
        None => {
            eprintln!("Error: Missing required argument --output");
            std::process::exit(1);
        }
    };

    println!("Input Text: {}", text);
    println!("Reference Audio: {}", reference);
    println!("Output WAV Path: {}", output);

    // Resolve paths dynamically
    let poc_dir = resolve_poc_dir();
    let export_script_path = poc_dir.join("export_onnx.py");
    let model_path = poc_dir.join("models").join("model.onnx");
    let voices_dir = poc_dir.join("voices");

    // Generate a unique voice name to prevent concurrency race conditions
    let pid = std::process::id();
    let voice_name = format!("temp_voice_{}", pid);
    let voice_filename = format!("{}.bin", voice_name);
    let voice_path = voices_dir.join(&voice_filename);

    // 1. Execute python script as subprocess to generate voice profile
    let python_exe = find_python_interpreter();
    println!("Executing {} export_onnx.py to extract voice style...", python_exe);
    let status = Command::new(&python_exe)
        .args([
            export_script_path.to_str().unwrap(),
            "--reference",
            &reference,
            "--voice-name",
            &voice_name,
        ])
        .status()?;

    if !status.success() {
        return Err(format!("export_onnx.py subprocess failed with status: {:?}", status.code()).into());
    }
    println!("Voice profile '{}' extracted successfully.", voice_filename);

    // 2. Load the generated voice profile
    let voice_bytes = std::fs::read(&voice_path)?;
    // Delete the unique file immediately after reading
    let _ = std::fs::remove_file(&voice_path);

    let len_rounded = (voice_bytes.len() / 4) * 4;
    let voice_bytes_aligned = &voice_bytes[..len_rounded];
    #[allow(clippy::manual_is_multiple_of)]
    let voice_data: Vec<f32> = if voice_bytes_aligned.as_ptr() as usize % std::mem::align_of::<f32>() == 0 {
        bytemuck::cast_slice(voice_bytes_aligned).to_vec()
    } else {
        voice_bytes_aligned
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()
    };

    // 3. Initialize ORT Session once
    println!("Initializing ORT Inference Session...");
    let mut session = Session::builder()?
        .with_intra_threads(2)?
        .with_inter_threads(1)?
        .commit_from_file(&model_path)?;

    // 4. Split text into chunks to prevent sequence limit crashes
    let chunks = split_text(&text);
    println!("Split text into {} chunk(s).", chunks.len());

    let mut combined_waveform = Vec::new();
    let tokenizer = tokenizer::TtsTokenizer::new();

    for (i, chunk) in chunks.iter().enumerate() {
        println!("Processing chunk {}/{}: \"{}\"", i + 1, chunks.len(), chunk);
        
        // Clean, phonemize and tokenize text chunk
        let phonemes = g2p::G2p::phonemize(chunk);
        let token_ids = tokenizer.tokenize(&phonemes);
        let seq_len = token_ids.len();

        // Extract style vector based on token sequence length
        let index = (seq_len.saturating_sub(2)).min(509);
        let offset = index * 256;
        if offset + 256 > voice_data.len() {
            return Err(format!(
                "Voice style offset {} out of bounds (voice data length: {})",
                offset + 256,
                voice_data.len()
            ).into());
        }
        let style_slice = &voice_data[offset..offset + 256];

        // Run ONNX Inference on chunk
        let inputs = ort::inputs![
            "input_ids" => Value::from_array((vec![1, seq_len], token_ids.to_vec()))?,
            "style" => Value::from_array((vec![1, 256], style_slice.to_vec()))?,
            "speed" => Value::from_array((vec![1], vec![1.0f32]))?,
        ];

        let outputs = session.run(inputs)?;
        let waveform_val = outputs
            .get("waveform")
            .ok_or("Missing waveform tensor in output")?;

        let (_, waveform_data) = waveform_val.try_extract_tensor::<f32>()?;
        let waveform_vec = waveform_data.to_vec();
        combined_waveform.extend(waveform_vec);
    }

    println!("Combined generated audio length (samples): {}", combined_waveform.len());

    // 5. Convert float values in [-1.0, 1.0] to 16-bit PCM and write output WAV
    println!("Converting floats to 16-bit PCM and writing to output file...");
    let pcm_data: Vec<i16> = combined_waveform
        .iter()
        .map(|&val| {
            let clamped = val.clamp(-1.0, 1.0);
            (clamped * 32767.0) as i16
        })
        .collect();

    let mut out_file = File::create(&output)?;
    let header = create_wav_header(pcm_data.len());
    out_file.write_all(&header)?;

    let mut pcm_bytes = Vec::with_capacity(pcm_data.len() * 2);
    for sample in pcm_data {
        pcm_bytes.extend_from_slice(&sample.to_le_bytes());
    }
    out_file.write_all(&pcm_bytes)?;

    println!("Audio synthesis complete. Output written to: {}", output);

    Ok(())
}
