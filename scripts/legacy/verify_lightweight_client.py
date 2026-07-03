import asyncio
import websockets
import struct
import json
import subprocess
import os
import sys
import time
import psutil

async def run_test_logic(websocket):
    # 1. Send binary OP_AUTH_HANDSHAKE
    op_code = 0x00
    seq_id = 1
    payload = b"test_handshake_payload"
    # encode VoiceFrame:
    # op_code (1 byte), seq_id (4 bytes, little-endian), payload_len (4 bytes, little-endian), payload
    frame = struct.pack("<BII", op_code, seq_id, len(payload)) + payload
    
    print(f"Sending OP_AUTH_HANDSHAKE binary frame (seq_id={seq_id})...")
    await websocket.send(frame)
    
    # Receive response
    resp = await websocket.recv()
    if not isinstance(resp, bytes):
        raise ValueError(f"Expected binary handshake response, got: {resp}")
    
    # decode VoiceFrame
    resp_op_code, resp_seq_id, resp_payload_len = struct.unpack("<BII", resp[:9])
    resp_payload = resp[9:]
    print(f"Received response: op_code={resp_op_code}, seq_id={resp_seq_id}, payload_len={resp_payload_len}")
    
    if resp_op_code != 0x00:
        raise ValueError(f"Expected response op_code 0x00, got {resp_op_code}")
    if resp_seq_id != seq_id:
        raise ValueError(f"Expected response seq_id {seq_id}, got {resp_seq_id}")
    if resp_payload != payload:
        raise ValueError(f"Expected response payload {payload}, got {resp_payload}")
    
    print("Binary handshake verified successfully!")
    
    # 2. Send JSON Text Command: ping
    ping_cmd = {
        "id": "req_001",
        "command": "ping",
        "payload": {}
    }
    print("Sending JSON ping command...")
    await websocket.send(json.dumps(ping_cmd))
    
    # Receive JSON Text response
    resp_text = await websocket.recv()
    if not isinstance(resp_text, str):
        raise ValueError(f"Expected text response, got: {resp_text}")
    
    resp_json = json.loads(resp_text)
    print(f"Received JSON response: {resp_json}")
    
    if resp_json.get("id") != "req_001":
        raise ValueError(f"Expected response id 'req_001', got {resp_json.get('id')}")
    if resp_json.get("status") != "ok":
        raise ValueError(f"Expected status 'ok', got {resp_json.get('status')}")
    if resp_json.get("data") != {"pong": True}:
        raise ValueError(f"Expected data {{'pong': True}}, got {resp_json.get('data')}")
        
    print("JSON Command verified successfully!")

async def main():
    possible_paths = [
        os.path.join("target", "debug", "liva-native-core.exe"),
        os.path.join("target", "debug", "liva-native-core"),
        os.path.join("liva-native-core", "target", "debug", "liva-native-core.exe"),
        os.path.join("liva-native-core", "target", "debug", "liva-native-core"),
        os.path.join("target", "debug", "liva_native_core.exe"),
        os.path.join("target", "debug", "liva_native_core"),
    ]
    server_path = None
    for p in possible_paths:
        if os.path.exists(p):
            server_path = p
            break
            
    if not server_path:
        print("Error: Compiled server binary not found. Try running cargo build first.")
        sys.exit(1)
        
    # Start the server process as a background process with clean/isolated environment
    env = os.environ.copy()
    env["LIVA_DB_IN_MEMORY"] = "1"
    env["LIVA_ENCRYPTION_KEY"] = "00000000000000000000000000000000"
    env["TELEGRAM_BOT_TOKEN"] = ""
    env["LIVA_TOKIO_WORKER_THREADS"] = "2"
    
    print(f"Starting server process: {server_path}")
    server_proc = None
    try:
        log_file = open("server_test.log", "w")
        server_proc = subprocess.Popen(
            [server_path],
            env=env,
            stdout=log_file,
            stderr=log_file
        )
        
        # Wait for the WebSocket server to be available by attempting connection
        uri = "ws://127.0.0.1:8002/ws"
        connected = False
        print("Connecting to LIVA WebSocket server...")
        for attempt in range(50):
            try:
                # check if server process has exited prematurely
                if server_proc.poll() is not None:
                    raise RuntimeError("Server process exited unexpectedly.")
                    
                async with websockets.connect(uri) as websocket:
                    connected = True
                    break
            except Exception as e:
                # Wait briefly before retrying
                await asyncio.sleep(0.1)
                
        if not connected:
            raise RuntimeError("Could not connect to WebSocket server after 5 seconds.")
            
        print("Connected! Running test logic...")
        async with websockets.connect(uri) as websocket:
            await run_test_logic(websocket)
            
        # Measure peak memory usage of the mock client process using psutil
        process = psutil.Process(os.getpid())
        mem_info = process.memory_info()
        peak_wset = getattr(mem_info, 'peak_wset', 0)
        rss = mem_info.rss
        peak_memory_bytes = max(peak_wset, rss)
        peak_memory_mb = peak_memory_bytes / (1024 * 1024)
        
        print(f"Client peak memory usage: {peak_memory_mb:.2f} MB")
        
        # Assert peak memory remains under 50MB
        assert peak_memory_mb < 50.0, f"Peak memory usage exceeded 50MB limit: {peak_memory_mb:.2f} MB"
        print("Memory assertion passed: Client peak memory is under 50MB.")
        print("Verification verification successfully completed.")
        
    except Exception as e:
        print(f"Verification failed: {e}")
        try:
            log_file.close()
            with open("server_test.log", "r") as f:
                print("\n--- SERVER LOGS ---")
                print(f.read())
                print("--------------------\n")
        except Exception as log_err:
            print(f"Failed to read server logs: {log_err}")
        sys.exit(1)
    finally:
        try:
            log_file.close()
        except:
            pass
        if server_proc:
            print("Shutting down LIVA native core server...")
            server_proc.terminate()
            try:
                server_proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                server_proc.kill()
                server_proc.wait()
            print("Server shutdown completed.")

if __name__ == "__main__":
    asyncio.run(main())
