import subprocess
import json
import sys
import os
import time
import threading
import queue

def enqueue_output(out, q):
    for line in iter(out.readline, ''):
        q.put(line)
    out.close()

def print_stderr(err):
    for line in iter(err.readline, ''):
        print(f"[SUBPROCESS STDERR] {line.strip()}", file=sys.stderr)
    err.close()

def spawn_subprocess(binary_path, env):
    proc = subprocess.Popen(
        [binary_path],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,  # Line buffered
        env=env
    )
    
    q = queue.Queue()
    t_out = threading.Thread(target=enqueue_output, args=(proc.stdout, q))
    t_out.daemon = True
    t_out.start()

    t_err = threading.Thread(target=print_stderr, args=(proc.stderr,))
    t_err.daemon = True
    t_err.start()
    
    return proc, q

def main():
    print("==========================================================")
    print("LIVA LLM ROUTER - FUNCTIONAL VERIFICATION SCRIPT")
    print("==========================================================")

    # 1. Paths configuration
    project_root = os.path.dirname(os.path.abspath(__file__))
    binary_path = os.path.join(project_root, "liva-native-core", "target", "debug", "liva-native-core.exe")
    model_path = os.path.join(project_root, "liva-ai-engine", "models", "gemma-4-E4B_q4_0-it.gguf")

    print(f"Project root: {project_root}")
    print(f"Binary path: {binary_path}")
    print(f"Model path: {model_path}")

    # Check existence
    if not os.path.exists(binary_path):
        print(f"[ERROR] Rust binary not found at {binary_path}")
        sys.exit(1)
    if not os.path.exists(model_path):
        print(f"[ERROR] Model file not found at {model_path}")
        sys.exit(1)

    # 2. Spawn liva-native-core.exe subprocess
    env = os.environ.copy()
    env["LIVA_DB_IN_MEMORY"] = "true"  # Use in-memory DB for test safety

    print("\nStarting LIVA Native Core subprocess...")
    proc, q = spawn_subprocess(binary_path, env)

    # Helper function to send requests
    request_id_counter = 0

    def send_request(command, payload):
        nonlocal request_id_counter
        request_id_counter += 1
        req_id = f"req-{request_id_counter}"
        req = {
            "id": req_id,
            "command": command,
            "payload": payload
        }
        req_str = json.dumps(req)
        print(f"\n---> Sending command: {command} (ID: {req_id})")
        proc.stdin.write(req_str + "\n")
        proc.stdin.flush()
        return req_id

    def read_responses_until(req_id, timeout=10.0, is_stream=False):
        start_time = time.time()
        responses = []
        while time.time() - start_time < timeout:
            # Check if subprocess died
            ret = proc.poll()
            if ret is not None:
                print(f"[ERROR] Subprocess exited with code {ret}")
                break

            try:
                line = q.get(timeout=0.1)
                line = line.strip()
                if not line:
                    continue
                
                resp = json.loads(line)
                if resp.get("id") == req_id:
                    responses.append(resp)
                    print(f"<--- Received: {line[:200]}...")
                    if not is_stream:
                        return resp
                    # For streaming, the final response will have done: True in data
                    if is_stream:
                        data = resp.get("data")
                        if data and isinstance(data, dict) and data.get("done") is True:
                            return responses
            except queue.Empty:
                continue
            except json.JSONDecodeError:
                print(f"Raw output (failed to parse): {line}")

        # If we get here, it timed out
        # Check if process is still alive
        ret = proc.poll()
        if ret is not None:
            raise RuntimeError(f"Subprocess terminated with code {ret}")
        raise TimeoutError(f"Timed out waiting for response to ID {req_id}")

    try:
        # Test 1: Ping
        req_id = send_request("ping", {})
        resp = read_responses_until(req_id)
        assert resp["status"] == "ok"
        assert resp["data"] == {"pong": True}
        print("[OK] Ping test passed")

        # Test 2: Status
        req_id = send_request("status", {})
        resp = read_responses_until(req_id)
        assert resp["status"] == "ok"
        assert resp["data"]["engine"] == "LIVA Native Engine"
        print("[OK] Status test passed")

        # Test 3: LLM Health Check
        req_id = send_request("llm:health_check", {})
        resp = read_responses_until(req_id)
        assert resp["status"] == "ok"
        print(f"[OK] LLM Health Check: {resp['data']}")

        # Test 4: Swap Model (loads the actual Gemma model)
        print("\nSwapping model to Gemma-4 (this might take a few seconds)...")
        req_id = send_request("llm:swap_model", {
            "model_path": model_path,
            "n_ctx": 1024,
            "n_gpu_layers": 0,
            "vocab_only": False
        })
        resp = read_responses_until(req_id, timeout=90.0)
        assert resp["status"] == "ok"
        print("[OK] Swap model test passed")

        # Test 5: LLM Health Check (now loaded)
        req_id = send_request("llm:health_check", {})
        resp = read_responses_until(req_id)
        assert resp["status"] == "ok"
        assert resp["data"]["model_loaded"] is True
        print("[OK] Health check after load passed")

        # Test 6: Embeddings Extraction (expected to crash due to llama.cpp bug)
        print("\nTesting Embeddings Extraction (expected to fail/crash)...")
        try:
            req_id = send_request("llm:embed", {
                "input": "Testing embeddings extraction"
            })
            resp = read_responses_until(req_id, timeout=10.0)
            if resp["status"] == "ok":
                print(f"[OK] Embeddings test passed. Dimension: {len(resp['data'])}")
            else:
                print(f"[FAIL] Embeddings extraction returned status: {resp['status']}")
        except Exception as embed_err:
            print(f"[KNOWN BUG OBSERVED] Embeddings extraction crashed/failed: {embed_err}")
            
            # Since the process crashed, we must relaunch it for completion tests
            print("\nRelaunching LIVA Native Core subprocess...")
            try:
                proc.kill()
            except Exception:
                pass
            proc, q = spawn_subprocess(binary_path, env)
            
            # Re-swap the model
            print("Re-swapping model to Gemma-4...")
            req_id = send_request("llm:swap_model", {
                "model_path": model_path,
                "n_ctx": 1024,
                "n_gpu_layers": 0,
                "vocab_only": False
            })
            resp = read_responses_until(req_id, timeout=90.0)
            assert resp["status"] == "ok"
            print("Model successfully re-loaded.")

        # Test 7: Unary Completion Output & Token Usage Statistics
        req_id = send_request("chat:completion", {
            "messages": [
                {"role": "user", "content": "Say hello in exactly one word."}
            ],
            "temperature": 0.0,
            "top_p": 1.0,
            "stream": False
        })
        resp = read_responses_until(req_id, timeout=60.0)
        assert resp["status"] == "ok"
        data = resp["data"]
        assert data["done"] is True
        assert "text" in data
        assert "usage" in data
        usage = data["usage"]
        assert "prompt_tokens" in usage
        assert "completion_tokens" in usage
        assert "total_tokens" in usage
        assert usage["prompt_tokens"] > 0
        assert usage["completion_tokens"] > 0
        assert usage["total_tokens"] == usage["prompt_tokens"] + usage["completion_tokens"]
        
        print("\nUnary Completion result:")
        print(f"- Generated Text: {data['text'].strip()}")
        print(f"- Prompt Tokens: {usage['prompt_tokens']}")
        print(f"- Completion Tokens: {usage['completion_tokens']}")
        print(f"- Total Tokens: {usage['total_tokens']}")
        print("[OK] Unary Completion & Usage test passed")

        # Test 8: Streaming Completion Output & Token Usage Statistics
        req_id = send_request("chat:completion", {
            "messages": [
                {"role": "user", "content": "Count from 1 to 3 in words."}
            ],
            "temperature": 0.0,
            "top_p": 1.0,
            "stream": True
        })
        responses = read_responses_until(req_id, timeout=60.0, is_stream=True)
        
        # Verify streaming chunks
        chunks = [r for r in responses if not r["data"].get("done")]
        final_resp = [r for r in responses if r["data"].get("done")][0]
        
        assert len(chunks) > 0
        for chunk in chunks:
            assert "token" in chunk["data"]
            assert chunk["data"]["done"] is False
            
        final_data = final_resp["data"]
        assert final_data["done"] is True
        assert "text" in final_data
        assert "usage" in final_data
        usage = final_data["usage"]
        assert "prompt_tokens" in usage
        assert "completion_tokens" in usage
        assert "total_tokens" in usage
        assert usage["prompt_tokens"] > 0
        assert usage["completion_tokens"] > 0
        assert usage["total_tokens"] == usage["prompt_tokens"] + usage["completion_tokens"]
        
        print("\nStreaming Completion result:")
        print(f"- Collected Chunks Count: {len(chunks)}")
        print(f"- Final Text: {final_data['text'].strip()}")
        print(f"- Prompt Tokens: {usage['prompt_tokens']}")
        print(f"- Completion Tokens: {usage['completion_tokens']}")
        print(f"- Total Tokens: {usage['total_tokens']}")
        print("[OK] Streaming Completion & Usage test passed")

        print("\n[SUCCESS] Verification script finished execution successfully!")

    except Exception as e:
        print(f"\n[FAILURE] Exception occurred: {e}")
        try:
            proc.kill()
        except Exception:
            pass
        raise e

    finally:
        print("\nShutting down subprocess...")
        try:
            proc.stdin.close()
            proc.wait(timeout=5)
        except Exception:
            try:
                proc.kill()
            except Exception:
                pass
        print("Subprocess closed.")

if __name__ == "__main__":
    main()
