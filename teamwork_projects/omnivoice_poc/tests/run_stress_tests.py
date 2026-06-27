import os
import sys
import subprocess
import numpy as np
import soundfile as sf
import traceback

# Reconfigure stdout/stderr for UTF-8 on Windows
if sys.platform.startswith('win'):
    import codecs
    sys.stdout.reconfigure(encoding='utf-8')
    sys.stderr.reconfigure(encoding='utf-8')


# Add the POC directory to sys.path to import export_onnx
sys.path.append(r"E:\Project\LIVA\teamwork_projects\omnivoice_poc")
import export_onnx

def run_cmd(args):
    print(f"Running command: {' '.join(args)}")
    res = subprocess.run(args, capture_output=True)
    stdout = res.stdout.decode('utf-8', errors='replace') if res.stdout else ""
    stderr = res.stderr.decode('utf-8', errors='replace') if res.stderr else ""
    return res.returncode, stdout, stderr

def main():
    print("=== OMNIVOICE POC STRESS TESTING HARNESS ===")
    
    test_dir = r"E:\Project\LIVA\teamwork_projects\omnivoice_poc\tests"
    os.makedirs(test_dir, exist_ok=True)
    
    # -------------------------------------------------------------
    # 1. Stress test the Python style extraction with various WAV formats
    # -------------------------------------------------------------
    print("\n--- PHASE 1: Stress testing Python style extraction ---")
    
    wav_formats = {
        "silent": {"data": np.zeros(16000 * 3, dtype=np.float32), "sr": 16000},
        "stereo": {"data": np.random.randn(16000 * 3, 2).astype(np.float32) * 0.1, "sr": 16000},
        "short_1s": {"data": np.random.randn(16000 * 1).astype(np.float32) * 0.1, "sr": 16000},
        "long_60s": {"data": np.random.randn(16000 * 60).astype(np.float32) * 0.1, "sr": 16000},
        "rate_8k": {"data": np.random.randn(8000 * 3).astype(np.float32) * 0.1, "sr": 8000},
        "rate_16k": {"data": np.random.randn(16000 * 3).astype(np.float32) * 0.1, "sr": 16000},
        "rate_44_1k": {"data": np.random.randn(44100 * 3).astype(np.float32) * 0.1, "sr": 44100},
        "rate_48k": {"data": np.random.randn(48000 * 3).astype(np.float32) * 0.1, "sr": 48000},
    }
    
    extraction_results = {}
    
    for name, config in wav_formats.items():
        path = os.path.join(test_dir, f"test_{name}.wav")
        # Write wav file
        sf.write(path, config["data"], config["sr"])
        
        print(f"\nTesting WAV: {name} (shape: {config['data'].shape}, sr: {config['sr']})")
        try:
            vector = export_onnx.extract_style_vector(path)
            shape = vector.shape
            has_nan = np.any(np.isnan(vector))
            has_inf = np.any(np.isinf(vector))
            mean_val = np.mean(vector)
            std_val = np.std(vector)
            
            print(f"  Result: Success! Shape: {shape}, Has NaN: {has_nan}, Has Inf: {has_inf}")
            print(f"  Stats: Mean = {mean_val:.4f}, Std = {std_val:.4f}")
            
            extraction_results[name] = {
                "success": True,
                "shape": shape,
                "has_nan": has_nan,
                "has_inf": has_inf,
                "mean": mean_val,
                "std": std_val
            }
        except Exception as e:
            print(f"  Result: FAILED with exception: {e}")
            traceback.print_exc()
            extraction_results[name] = {
                "success": False,
                "error": str(e)
            }
            
    # Clean up generated test wav files from python tests
    for name in wav_formats.keys():
        path = os.path.join(test_dir, f"test_{name}.wav")
        if os.path.exists(path):
            try:
                os.remove(path)
            except Exception:
                pass

    # -------------------------------------------------------------
    # 2. Stress test the G2P and Tokenizer inside rust_cli
    # -------------------------------------------------------------
    print("\n--- PHASE 2: Stress testing G2P and Tokenizer in rust_cli ---")
    
    # We must ensure we clean the voices directory of any custom voices first to avoid validator bugs
    voices_dir = r"E:\Project\LIVA\teamwork_projects\omnivoice_poc\voices"
    predefined_voices = {
        'af_alloy.bin', 'af_aoede.bin', 'af_bella.bin', 'af_heart.bin', 'af_jessica.bin', 'af_kore.bin',
        'af_nicole.bin', 'af_nova.bin', 'af_river.bin', 'af_sarah.bin', 'af_sky.bin', 'am_adam.bin',
        'am_echo.bin', 'am_eric.bin', 'am_fenrir.bin', 'am_liam.bin', 'am_michael.bin', 'am_onyx.bin',
        'am_puck.bin', 'am_santa.bin', 'bf_alice.bin', 'bf_emma.bin', 'bf_isabella.bin', 'bf_lily.bin',
        'bm_daniel.bin', 'bm_fable.bin', 'bm_george.bin', 'bm_lewis.bin', 'ef_dora.bin', 'em_alex.bin',
        'em_santa.bin', 'ff_siwis.bin', 'hf_alpha.bin', 'hf_beta.bin', 'hm_omega.bin', 'hm_psi.bin',
        'if_sara.bin', 'im_nicola.bin', 'jf_alpha.bin', 'jf_gongitsune.bin', 'jf_nezumi.bin', 'jf_tebukuro.bin',
        'jm_kumo.bin', 'pf_dora.bin', 'pm_alex.bin', 'pm_santa.bin', 'zf_xiaobei.bin', 'zf_xiaoni.bin',
        'zf_xiaoxiao.bin', 'zf_xiaoyi.bin', 'zm_yunjian.bin', 'zm_yunxi.bin', 'zm_yunxia.bin', 'zm_yunyang.bin'
    }
    
    def cleanup_custom_voices():
        if os.path.exists(voices_dir):
            for f in os.listdir(voices_dir):
                if f not in predefined_voices:
                    try:
                        os.remove(os.path.join(voices_dir, f))
                    except Exception:
                        pass
                        
    cleanup_custom_voices()
    
    # We will use the models/asr_example.wav as the reference audio
    ref_wav = r"E:\Project\LIVA\models\asr_example.wav"
    cli_path = r"E:\Project\LIVA\teamwork_projects\omnivoice_poc\rust_cli\target\debug\omnivoice_rust_cli.exe"
    out_wav = os.path.join(test_dir, "rust_output.wav")
    
    texts_to_test = {
        "empty": "",
        "emojis": "😀😃😄😁😆😅😂🤣😊😇🙂🙃😉",
        "mixed_emojis": "Hello 👋 there! How is it going? 🚀🔥",
        "non_ascii": "Chào thế giới! Đây là tiếng Việt. Русский текст, 日本語のテキスト, 汉语测试。",
        "extremely_long": "Hello world. " * 800, # 9600 chars
    }
    
    rust_results = {}
    
    for label, text in texts_to_test.items():
        print(f"\nTesting text: {label} (len: {len(text)})")
        cleanup_custom_voices() # clean before run
        if os.path.exists(out_wav):
            try:
                os.remove(out_wav)
            except Exception:
                pass
                
        cmd = [cli_path, "--text", text, "--reference", ref_wav, "--output", out_wav]
        code, stdout, stderr = run_cmd(cmd)
        
        print(f"  Exit code: {code}")
        if code != 0:
            print(f"  Stdout:\n{stdout}")
            print(f"  Stderr:\n{stderr}")
            rust_results[label] = {
                "success": False,
                "exit_code": code,
                "stdout": stdout,
                "stderr": stderr
            }
        else:
            # Check output wav properties
            if os.path.exists(out_wav):
                try:
                    data, sr = sf.read(out_wav)
                    dur = len(data) / sr
                    print(f"  Result: Success! Generated audio length: {len(data)} samples ({dur:.3f}s), sr: {sr}")
                    rust_results[label] = {
                        "success": True,
                        "samples": len(data),
                        "duration_sec": dur,
                        "sr": sr
                    }
                except Exception as e:
                    print(f"  Result: FAILED to read output wav: {e}")
                    rust_results[label] = {
                        "success": False,
                        "error": f"Failed to read output wav: {e}"
                    }
            else:
                print("  Result: FAILED (Exit code 0 but no output file generated!)")
                rust_results[label] = {
                    "success": False,
                    "error": "No output wav file generated"
                }

    # -------------------------------------------------------------
    # 3. Verify synthesized audio output duration matches the length of the input text string
    # -------------------------------------------------------------
    print("\n--- PHASE 3: Verifying output duration vs input text length ---")
    
    # We will test text lengths of 10, 50, 100, 200, 300, 500, 1000 characters
    duration_tests = [
        "Short text",
        "This is a slightly longer sentence designed to test output audio duration scaling.",
        "This is a slightly longer sentence designed to test output audio duration scaling. " * 2,
        "This is a slightly longer sentence designed to test output audio duration scaling. " * 4,
        "This is a slightly longer sentence designed to test output audio duration scaling. " * 6,
        "This is a slightly longer sentence designed to test output audio duration scaling. " * 10,
    ]
    
    durations = []
    
    for i, txt in enumerate(duration_tests):
        cleanup_custom_voices()
        if os.path.exists(out_wav):
            try:
                os.remove(out_wav)
            except Exception:
                pass
                
        cmd = [cli_path, "--text", txt, "--reference", ref_wav, "--output", out_wav]
        code, stdout, stderr = run_cmd(cmd)
        
        if code == 0 and os.path.exists(out_wav):
            data, sr = sf.read(out_wav)
            dur = len(data) / sr
            char_len = len(txt)
            # Count phonemes and tokens from stdout to get more details
            phonemes = ""
            for line in stdout.splitlines():
                if line.startswith("Phonemes: "):
                    phonemes = line[len("Phonemes: "):]
            
            durations.append({
                "char_len": char_len,
                "phoneme_len": len(phonemes),
                "duration_sec": dur,
                "samples": len(data)
            })
            print(f"Text chars: {char_len} | Phonemes: {len(phonemes)} | Duration: {dur:.3f}s")
        else:
            print(f"Failed to synthesize for text length {len(txt)}")
            
    # Clean up final output.wav
    if os.path.exists(out_wav):
        try:
            os.remove(out_wav)
        except Exception:
            pass
            
    # Print summary
    print("\n=== SUMMARY OF RESULTS ===")
    print("\n1. Extraction tests:")
    for k, v in extraction_results.items():
        if v["success"]:
            print(f"  {k:15}: SUCCESS (mean={v['mean']:.4f}, std={v['std']:.4f})")
        else:
            print(f"  {k:15}: FAILED ({v['error']})")
            
    print("\n2. Rust CLI boundary tests:")
    for k, v in rust_results.items():
        if v["success"]:
            print(f"  {k:15}: SUCCESS ({v['duration_sec']:.3f}s)")
        else:
            err = v.get('error') or f"Exit code {v.get('exit_code')}"
            print(f"  {k:15}: FAILED ({err})")
            
    print("\n3. Duration scaling:")
    for d in durations:
        print(f"  Chars: {d['char_len']:4} | Phonemes: {d['phoneme_len']:4} | Duration: {d['duration_sec']:.3f}s | Ratio (samples/char): {d['samples']/d['char_len']:.1f}")

if __name__ == "__main__":
    main()
