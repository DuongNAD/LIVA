import os
import sys
import time
import json
import struct
import asyncio
import subprocess
import psutil
import websockets

# Constants
OP_AUTH_HANDSHAKE = 0x00
SERVER_PORT = 8002
SERVER_HOST = "127.0.0.1"
WS_URI = f"ws://{SERVER_HOST}:{SERVER_PORT}/ws"
MEMORY_LIMIT_BYTES = 50 * 1024 * 1024  # 50MB

def find_binary():
    candidate_paths = [
        r"E:\Project\LIVA\target\debug\liva-native-core.exe",
        r"E:\Project\LIVA\target\debug\deps\liva_native_core.exe",
        r"target\debug\liva-native-core.exe"
    ]
    for path in candidate_paths:
        abs_path = os.path.abspath(path)
        if os.path.exists(abs_path):
            return abs_path
    raise FileNotFoundError("Could not find Rust server binary liva-native-core.exe in target directories")

async def run_verification():
    binary_path = find_binary()
    print(f"[Verification] Found Rust server binary at: {binary_path}")

    # Set up environment variables
    env = os.environ.copy()
    env["LIVA_DB_IN_MEMORY"] = "1"
    env["LIVA_STT_MODEL_DIR"] = "non_existent_dir"
    env["LIVA_TTS_MODEL_PATH"] = "non_existent_path"
    env["LIVA_SERVER_PORT"] = str(SERVER_PORT)
    env["LIVA_SERVER_HOST"] = SERVER_HOST
    env["LIVA_TOKIO_WORKER_THREADS"] = "2" # Minimize resource usage for verification

    print("[Verification] Spawning server process...")
    cwd_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "src")
    if not os.path.exists(cwd_path):
        os.makedirs(cwd_path, exist_ok=True)
    proc = subprocess.Popen(
        [binary_path],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        cwd=cwd_path
    )

    websocket = None
    try:
        # Give server a moment to bind
        print("[Verification] Waiting for server to bind port...")
        await asyncio.sleep(3)

        # Connect to server
        print(f"[Verification] Connecting to {WS_URI}...")
        for attempt in range(20):
            try:
                websocket = await asyncio.wait_for(websockets.connect(WS_URI, close_timeout=2), timeout=5.0)
                print("[Verification] Connected successfully!")
                break
            except Exception as e:
                if attempt == 19:
                    raise RuntimeError(f"Could not connect to WebSocket after 20 attempts: {e}")
                print(f"[Verification] Connection attempt {attempt + 1} failed, retrying in 1s...")
                await asyncio.sleep(1.0)

        # 1. Verify OP_AUTH_HANDSHAKE
        print("[Verification] Testing OP_AUTH_HANDSHAKE binary frame...")
        token_payload = b"verification_token_s24"
        seq_id = 42
        # Header layout (9 bytes): 1-byte opcode, 4-byte little-endian seq_id, 4-byte little-endian payload_size
        header = struct.pack("<BII", OP_AUTH_HANDSHAKE, seq_id, len(token_payload))
        frame_to_send = header + token_payload
        
        await websocket.send(frame_to_send)
        print("[Verification] Sent binary handshake frame.")

        response = await asyncio.wait_for(websocket.recv(), timeout=5.0)
        if not isinstance(response, bytes):
            raise ValueError(f"Handshake response is not binary (got type: {type(response)})")
        
        if len(response) < 9:
            raise ValueError(f"Response length {len(response)} is too short for 9-byte header")

        resp_opcode, resp_seq, resp_size = struct.unpack("<BII", response[:9])
        resp_payload = response[9:]

        print(f"[Verification] Received frame: OpCode={resp_opcode}, SeqId={resp_seq}, Size={resp_size}, Payload={resp_payload}")
        
        if resp_opcode != OP_AUTH_HANDSHAKE:
            raise ValueError(f"Opcode mismatch: expected {OP_AUTH_HANDSHAKE}, got {resp_opcode}")
        if resp_seq != seq_id:
            raise ValueError(f"SeqId mismatch: expected {seq_id}, got {resp_seq}")
        if resp_payload != token_payload:
            raise ValueError(f"Payload mismatch: expected {token_payload}, got {resp_payload}")
        
        print("[Verification] OP_AUTH_HANDSHAKE verified successfully!")

        # 2. Verify JSON Ping
        print("[Verification] Testing JSON Ping command...")
        ping_req_id = "test-ping-verification"
        ping_cmd = {
            "id": ping_req_id,
            "command": "ping",
            "payload": {}
        }
        await websocket.send(json.dumps(ping_cmd))
        print("[Verification] Sent JSON Ping command.")

        ping_resp_text = await asyncio.wait_for(websocket.recv(), timeout=5.0)
        if not isinstance(ping_resp_text, str):
            raise ValueError("Expected JSON text response for ping command")
        
        ping_resp = json.loads(ping_resp_text)
        print(f"[Verification] Received JSON response: {ping_resp}")
        
        if ping_resp.get("id") != ping_req_id:
            raise ValueError(f"Response ID mismatch: expected '{ping_req_id}', got '{ping_resp.get('id')}'")
        if ping_resp.get("status") != "ok":
            raise ValueError(f"Response status not 'ok': {ping_resp.get('error')}")
        if ping_resp.get("data") != {"pong": True}:
            raise ValueError(f"Response data mismatch: expected {{'pong': True}}, got {ping_resp.get('data')}")

        print("[Verification] JSON Ping verified successfully!")

        # 3. Verify peak memory usage
        print("[Verification] Measuring server memory usage...")
        p = psutil.Process(proc.pid)
        memory_info = p.memory_info()
        rss_bytes = memory_info.rss
        rss_mb = rss_bytes / (1024 * 1024)
        print(f"[Verification] Server Peak RSS: {rss_mb:.2f} MB ({rss_bytes} bytes)")
        
        if rss_bytes > MEMORY_LIMIT_BYTES:
            raise ValueError(f"Peak memory footprint {rss_mb:.2f} MB exceeds 50MB limit!")
        print("[Verification] Memory footprint is within safe bounds (<50MB).")

    except Exception as e:
        print(f"[Verification] ERROR: {e}", file=sys.stderr)
        sys.exit(1)
    finally:
        if websocket:
            print("[Verification] Closing websocket...")
            await websocket.close()
        
        # Terminate server
        print("[Verification] Terminating server subprocess...")
        proc.terminate()
        try:
            proc.wait(timeout=3)
            print("[Verification] Server terminated successfully.")
        except subprocess.TimeoutExpired:
            print("[Verification] Terminate timed out. Killing server...")
            proc.kill()
            proc.wait()
            print("[Verification] Server killed.")

        # Print outputs
        stdout, stderr = proc.communicate()
        if stdout:
            print(f"--- Server stdout ---\n{stdout.decode(errors='replace')}")
        if stderr:
            print(f"--- Server stderr ---\n{stderr.decode(errors='replace')}")

    print("[Verification] SUCCESS! All verification checks passed.")

if __name__ == "__main__":
    asyncio.run(run_verification())
