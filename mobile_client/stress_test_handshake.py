import os
import sys
import time
import json
import struct
import asyncio
import subprocess
import psutil
import websockets

OP_AUTH_HANDSHAKE = 0x00
SERVER_PORT = 8003  # Use a different port to avoid conflict
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
    raise FileNotFoundError("Could not find Rust server binary liva-native-core.exe")

async def test_single_client(client_id):
    print(f"[Stress-{client_id}] Connecting to {WS_URI}...")
    async with websockets.connect(WS_URI, close_timeout=2) as ws:
        print(f"[Stress-{client_id}] Connected!")

        # 1. Verify OP_AUTH_HANDSHAKE
        token = f"token_{client_id}".encode()
        seq = client_id * 100
        header = struct.pack("<BII", OP_AUTH_HANDSHAKE, seq, len(token))
        await ws.send(header + token)
        resp = await ws.recv()
        assert isinstance(resp, bytes), "Response must be binary"
        resp_op, resp_seq, resp_size = struct.unpack("<BII", resp[:9])
        resp_payload = resp[9:]
        assert resp_op == OP_AUTH_HANDSHAKE, f"Opcode mismatch: {resp_op}"
        assert resp_seq == seq, f"Seq mismatch: {resp_seq}"
        assert resp_payload == token, f"Payload mismatch: {resp_payload}"
        print(f"[Stress-{client_id}] Handshake OK")

        # 2. Verify JSON Ping
        ping_cmd = {
            "id": f"ping-{client_id}",
            "command": "ping",
            "payload": {}
        }
        await ws.send(json.dumps(ping_cmd))
        resp_text = await ws.recv()
        assert isinstance(resp_text, str), "Response must be string"
        resp_json = json.loads(resp_text)
        assert resp_json.get("id") == f"ping-{client_id}"
        assert resp_json.get("status") == "ok"
        assert resp_json.get("data") == {"pong": True}
        print(f"[Stress-{client_id}] Ping OK")

        # 3. Malformed JSON test
        await ws.send("{ malformed_json ")
        resp_err_text = await ws.recv()
        resp_err = json.loads(resp_err_text)
        assert resp_err.get("id") == "unknown"
        assert resp_err.get("status") == "error"
        assert "Invalid JSON query" in resp_err.get("error", "")
        print(f"[Stress-{client_id}] Malformed JSON handling OK")

        # 4. Unknown command test
        unknown_cmd = {
            "id": f"unknown-{client_id}",
            "command": "not_a_real_command",
            "payload": {}
        }
        await ws.send(json.dumps(unknown_cmd))
        resp_unknown_text = await ws.recv()
        resp_unknown = json.loads(resp_unknown_text)
        assert resp_unknown.get("id") == f"unknown-{client_id}"
        assert resp_unknown.get("status") == "error"
        assert "Unknown command" in resp_unknown.get("error", "")
        print(f"[Stress-{client_id}] Unknown command handling OK")

        # 5. Invalid binary frame test (too large payload size)
        bad_header = struct.pack("<BII", OP_AUTH_HANDSHAKE, seq + 1, 2 * 1024 * 1024)
        await ws.send(bad_header + b"x" * 100)
        # Server should break frame decode, but connection should remain open.
        # Let's verify connection is still alive by sending a successful ping.
        ping_cmd2 = {
            "id": f"ping-after-bad-{client_id}",
            "command": "ping",
            "payload": {}
        }
        await ws.send(json.dumps(ping_cmd2))
        resp_text2 = await ws.recv()
        resp_json2 = json.loads(resp_text2)
        assert resp_json2.get("id") == f"ping-after-bad-{client_id}"
        assert resp_json2.get("status") == "ok"
        print(f"[Stress-{client_id}] Invalid binary frame handling OK (connection recovered)")

async def run_stress_test():
    binary_path = find_binary()
    env = os.environ.copy()
    env["LIVA_DB_IN_MEMORY"] = "1"
    env["LIVA_STT_MODEL_DIR"] = "non_existent_dir"
    env["LIVA_TTS_MODEL_PATH"] = "non_existent_path"
    env["LIVA_SERVER_PORT"] = str(SERVER_PORT)
    env["LIVA_SERVER_HOST"] = SERVER_HOST
    env["LIVA_TOKIO_WORKER_THREADS"] = "4" # Give it more threads for concurrency

    print("[Stress] Spawning server process...")
    cwd_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "src")
    os.makedirs(cwd_path, exist_ok=True)
    proc = subprocess.Popen(
        [binary_path],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        cwd=cwd_path
    )

    try:
        print("[Stress] Waiting for server to bind...")
        await asyncio.sleep(2)

        # Launch 3 concurrent clients with a small delay to avoid socket contention
        tasks = []
        for i in range(3):
            tasks.append(asyncio.create_task(test_single_client(i)))
            await asyncio.sleep(0.1)
        await asyncio.gather(*tasks)

        # Measure memory
        p = psutil.Process(proc.pid)
        rss_bytes = p.memory_info().rss
        rss_mb = rss_bytes / (1024 * 1024)
        print(f"[Stress] Server Peak RSS under load: {rss_mb:.2f} MB ({rss_bytes} bytes)")
        
        if rss_bytes > MEMORY_LIMIT_BYTES:
            raise ValueError(f"Peak memory footprint {rss_mb:.2f} MB exceeds 50MB limit!")
        print("[Stress] Memory footprint is within safe bounds.")

    finally:
        print("[Stress] Terminating server...")
        proc.terminate()
        try:
            proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
        print("[Stress] Server terminated.")
        
        stdout, stderr = proc.communicate()
        if stderr:
            print(f"--- Server stderr ---\n{stderr.decode(errors='replace')}")

if __name__ == "__main__":
    asyncio.run(run_stress_test())
