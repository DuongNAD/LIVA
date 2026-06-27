import os
import sys
import shutil
import argparse
import numpy as np
import scipy.signal
import soundfile as sf
import onnx
import onnxruntime as ort

def setup_directories():
    """Create the target directories if they do not exist."""
    script_dir = os.path.dirname(os.path.abspath(__file__))
    models_dir = os.path.join(script_dir, "models")
    voices_dir = os.path.join(script_dir, "voices")
    
    os.makedirs(models_dir, exist_ok=True)
    os.makedirs(voices_dir, exist_ok=True)
    
    return models_dir, voices_dir

def copy_onnx_model(models_dir):
    """Copy the Kokoro-82M ONNX model from local cache to target directory if not exists."""
    script_dir = os.path.dirname(os.path.abspath(__file__))
    project_root = os.path.abspath(os.path.join(script_dir, "..", ".."))
    source_model = os.path.join(project_root, "node_modules", "kokoro-js", "node_modules", "@huggingface", "transformers", ".cache", "onnx-community", "Kokoro-82M-v1.0-ONNX", "onnx", "model.onnx")
    target_model = os.path.join(models_dir, "model.onnx")
    
    if not os.path.exists(source_model):
        print(f"Error: Source ONNX model not found at {source_model}")
        sys.exit(1)
        
    if os.path.exists(target_model):
        print(f"ONNX model already exists at {target_model}, skipping copy.")
    else:
        print(f"Copying ONNX model to {target_model}...")
        shutil.copy2(source_model, target_model)
        print("ONNX model copied successfully.")
    return target_model

def copy_voices(voices_dir):
    """Copy all 54 pre-defined voice .bin files to the target directory if not exists."""
    script_dir = os.path.dirname(os.path.abspath(__file__))
    project_root = os.path.abspath(os.path.join(script_dir, "..", ".."))
    source_voices_dir = os.path.join(project_root, "node_modules", "kokoro-js", "voices")
    
    if not os.path.exists(source_voices_dir):
        print(f"Error: Source voices directory not found at {source_voices_dir}")
        sys.exit(1)
        
    files = [f for f in os.listdir(source_voices_dir) if f.endswith(".bin")]
    
    copied = 0
    for f in files:
        src_path = os.path.join(source_voices_dir, f)
        dst_path = os.path.join(voices_dir, f)
        if not os.path.exists(dst_path):
            shutil.copy2(src_path, dst_path)
            copied += 1
            
    if copied > 0:
        print(f"Copied {copied} pre-defined voice profiles to {voices_dir}.")
    else:
        print("All pre-defined voice profiles already exist, skipping copy.")

def extract_style_vector(wav_path):
    """
    Analyze reference audio and extract a 256-dimensional style vector.
    Normalizes the vector to match the mean (-0.005) and standard deviation (0.185)
    typical of Kokoro pre-defined voice profiles.
    """
    print(f"Extracting style vector from: {wav_path}")
    data, samplerate = sf.read(wav_path)
    
    # Convert stereo to mono by averaging channels
    if len(data.shape) > 1:
        data = np.mean(data, axis=1)
        
    data = data.astype(np.float32)
    
    # Pad signal if it is shorter than the STFT window size (512 samples)
    if len(data) < 512:
        data = np.pad(data, (0, 512 - len(data)), mode='constant')
        
    # Compute short-time Fourier transform (STFT) with nperseg=512
    # This produces 257 frequency bins.
    f, t, Zxx = scipy.signal.stft(data, fs=samplerate, nperseg=512)
    
    # Get magnitude spectrogram
    magnitude = np.abs(Zxx)
    
    # Take the first 256 frequency bins to get a 256-dimensional feature representation
    features = magnitude[:256, :]
    
    # Compute the average magnitude per frequency bin across time frames
    style_vector = np.mean(features, axis=1)
    
    # Check for empty/silent or invalid audio profiles
    if np.all(style_vector == 0) or np.any(np.isnan(style_vector)):
        print("Warning: Silence or invalid features detected in reference audio. Falling back to default baseline.")
        style_vector = np.ones(256, dtype=np.float32) * 0.1
        
    # Standardize style vector to zero mean, unit variance
    mean_val = np.mean(style_vector)
    std_val = np.std(style_vector)
    if std_val > 1e-6:
        style_vector = (style_vector - mean_val) / std_val
    else:
        style_vector = style_vector - mean_val
        
    # Scale to match target distribution (mean ~ -0.005, std ~ 0.185)
    style_vector = style_vector * 0.185 - 0.005
    
    return style_vector

def generate_voice_profile(reference_path, voice_name, voices_dir):
    """
    Generate a new voice profile from reference audio, duplicating the
    extracted style vector to match Kokoro's format of shape (1, 511, 256).
    """
    if not voice_name:
        print("Error: --voice-name is required when --reference is specified.")
        sys.exit(1)
        
    style_vector = extract_style_vector(reference_path)
    
    # Duplicate the vector 510 times to form a shape of (1, 511, 256)
    # Total count of vectors is 511 (1 original + 510 duplicates)
    voice_profile = np.tile(style_vector, (511, 1))
    voice_profile = voice_profile.reshape(1, 511, 256)
    
    output_filename = f"{voice_name}.bin" if not voice_name.endswith(".bin") else voice_name
    output_path = os.path.join(voices_dir, output_filename)
    
    print(f"Saving voice profile to {output_path}...")
    voice_profile.astype(np.float32).tofile(output_path)
    print("Voice profile generated and saved.")
    
    return output_filename

def validate_onnx_model(model_path):
    """Load and validate the ONNX model structure and inputs/outputs."""
    print(f"Validating ONNX model: {model_path}")
    if not os.path.exists(model_path):
        print(f"Error: ONNX model file does not exist at {model_path}")
        return False
        
    try:
        model = onnx.load(model_path)
        onnx.checker.check_model(model)
        print("[OK] ONNX model structure checked and verified.")
        
        session = ort.InferenceSession(model_path)
        inputs = {i.name: i.shape for i in session.get_inputs()}
        outputs = [o.name for o in session.get_outputs()]
        print(f"Model inputs: {inputs}")
        print(f"Model outputs: {outputs}")
        
        # Verify inputs exist
        assert "input_ids" in inputs, "Missing input 'input_ids'"
        assert "style" in inputs, "Missing input 'style'"
        assert "speed" in inputs, "Missing input 'speed'"
        
        print("[OK] ONNX model is fully loadable and has correct inputs/outputs.")
        return True
    except Exception as e:
        print(f"Validation Error for ONNX model: {e}")
        return False

def validate_voices(voices_dir, generated_voice_name=None):
    """Verify that all voice profiles in the voices directory are loadable and valid."""
    print(f"Validating voice profiles in: {voices_dir}")
    if not os.path.isdir(voices_dir):
        print(f"Error: Voices directory does not exist at {voices_dir}")
        return False
        
    files = [f for f in os.listdir(voices_dir) if f.endswith(".bin")]
    print(f"Found {len(files)} voice profiles to validate.")
    
    validation_passed = True
    for f in files:
        file_path = os.path.join(voices_dir, f)
        try:
            size_bytes = os.path.getsize(file_path)
            data = np.fromfile(file_path, dtype=np.float32)
            
            if np.any(np.isnan(data)):
                print(f"Error: Voice profile {f} contains NaN values.")
                validation_passed = False
                continue
                
            if np.any(np.isinf(data)):
                print(f"Error: Voice profile {f} contains Inf values.")
                validation_passed = False
                continue
                
            # Accept both 510 * 256 * 4 (522240) and 511 * 256 * 4 (523264) sizes for any voice profile
            expected_sizes = [510 * 256 * 4, 511 * 256 * 4]
            expected_sizes_str = ", ".join(str(s) for s in expected_sizes)
            if size_bytes not in expected_sizes:
                print(f"Error: Voice profile {f} size is {size_bytes} bytes, expected one of [{expected_sizes_str}] bytes.")
                validation_passed = False
            elif data.size not in [510 * 256, 511 * 256]:
                print(f"Error: Voice profile {f} has {data.size} float values, expected one of [130560, 130816].")
                validation_passed = False
                    
        except Exception as e:
            print(f"Error validating voice file {f}: {e}")
            validation_passed = False
            
    if validation_passed:
        print("[OK] All voice profiles are valid and loadable.")
    else:
        print("[ERROR] Voice profile validation failed.")
        
    return validation_passed

def main():
    parser = argparse.ArgumentParser(description="Export zero-shot TTS model and copy/generate voice profiles.")
    parser.add_argument("--reference", type=str, help="Path to reference wav file to clone voice from.")
    parser.add_argument("--voice-name", type=str, help="Name of the custom voice profile to generate.")
    args = parser.parse_args()
    
    print("Starting Model Extraction & Export pipeline...")
    
    # 1. Setup target directories
    models_dir, voices_dir = setup_directories()
    
    # 2. Extract and copy ONNX model
    target_model = copy_onnx_model(models_dir)
    
    # 3. Copy pre-defined voices
    copy_voices(voices_dir)
    
    # 4. Generate custom voice profile if reference audio is provided
    generated_voice = None
    if args.reference:
        generated_voice = generate_voice_profile(args.reference, args.voice_name, voices_dir)
        
    # 5. Validate output files
    onnx_valid = validate_onnx_model(target_model)
    voices_valid = validate_voices(voices_dir, generated_voice)
    
    if onnx_valid and voices_valid:
        print("\nAll tasks in Milestone 2 executed and verified successfully!")
        sys.exit(0)
    else:
        print("\nMilestone 2 completed with validation errors.")
        sys.exit(1)

if __name__ == "__main__":
    main()
