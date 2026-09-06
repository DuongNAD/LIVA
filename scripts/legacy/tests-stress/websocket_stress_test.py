import asyncio
import websockets
import struct
import json
import subprocess
import os
import sys
import time
import psutil

# VoiceFrame encoding constants
OP_AUTH_HANDSHAKE = 0x00
OP_MIC_IN = 0x01

async def test_malformed_json(uri):
    print("\n--- Running Test 1: Malformed JSON Text Frames ---")
    async with websockets.connect(uri) as ws:
        # Scenario 1: Unterminated JSON
        print("Sending unterminated JSON...")
        await ws.send('{"invalid_json": ')
        try:
            resp = await asyncio.wait_for(ws.recv(), timeout=1.0)
            print(f"Received response: {resp}")
            data = json.loads(resp)
            assert data.get("status") == "error", f"Expected error, got {data}"
            assert "Invalid JSON query" in data.get("error", ""), f"Unexpected error msg: {data}"
        except asyncio.TimeoutError:
            print("FAILED: No response received for unterminated JSON")
            raise

        # Scenario 2: Completely random non-JSON string
        print("Sending non-JSON garbage...")
        await ws.send('hello world, this is garbage text frame')
        try:
            resp = await asyncio.wait_for(ws.recv(), timeout=1.0)
            print(f"Received response: {resp}")
            data = json.loads(resp)
            assert data.get("status") == "error", f"Expected error, got {data}"
        except asyncio.TimeoutError:
            print("FAILED: No response received for garbage text")
            raise

        # Scenario 3: Missing command field
        print("Sending missing command field...")
        await ws.send(json.dumps({"id": "req_err_1", "payload": {}}))
        try:
            resp = await asyncio.wait_for(ws.recv(), timeout=1.0)
            print(f"Received response: {resp}")
            data = json.loads(resp)
            assert data.get("status") == "error", f"Expected error, got {data}"
        except asyncio.TimeoutError:
            print("FAILED: No response received for missing command")
            raise

        # Scenario 4: Command payload with wrong types
        print("Sending command with wrong types...")
        await ws.send(json.dumps({"id": 12345, "command": "ping", "payload": "not_an_object"}))
        try:
            resp = await asyncio.wait_for(ws.recv(), timeout=1.0)
            print(f"Received response: {resp}")
            data = json.loads(resp)
            assert data.get("status") == "error", f"Expected error, got {data}"
        except asyncio.TimeoutError:
            print("FAILED: No response received for wrong type")
            raise

        # Scenario 5: Valid command following errors (ensure connection is still active and healthy)
        print("Sending valid ping command to check session recovery...")
        await ws.send(json.dumps({"id": "req_recovery", "command": "ping", "payload": {}}))
        try:
            resp = await asyncio.wait_for(ws.recv(), timeout=1.0)
            print(f"Received response: {resp}")
            data = json.loads(resp)
            assert data.get("id") == "req_recovery" and data.get("status") == "ok", f"Expected ok ping, got {data}"
            print("Recovery check passed: connection remained active.")
        except asyncio.TimeoutError:
            print("FAILED: Session did not recover after errors")
            raise

async def test_high_frequency_commands(uri):
    print("\n--- Running Test 2: High Frequency of Rapid Commands ---")
    
    num_clients = 50
    commands_per_client = 10
    
    print(f"Opening {num_clients} concurrent client connections...")
    
    async def single_client_session(client_id):
        async with websockets.connect(uri) as ws:
            # 1. Binary Handshake
            handshake_payload = f"hs_client_{client_id}".encode()
            frame = struct.pack("<BII", OP_AUTH_HANDSHAKE, client_id, len(handshake_payload)) + handshake_payload
            await ws.send(frame)
            resp = await ws.recv()
            assert isinstance(resp, bytes), f"Client {client_id} expected binary response"
            
            # 2. Sequential text commands
            for i in range(commands_per_client):
                if i % 2 == 0:
                    cmd = {"id": f"ping_{client_id}_{i}", "command": "ping", "payload": {}}
                else:
                    cmd = {"id": f"get_tasks_{client_id}_{i}", "command": "get_tasks", "payload": {}}
                    
                await ws.send(json.dumps(cmd))
                resp_text = await ws.recv()
                data = json.loads(resp_text)
                assert data.get("id") == cmd["id"]
                assert data.get("status") == "ok"
                
    t0 = time.time()
    tasks = [single_client_session(i) for i in range(num_clients)]
    await asyncio.gather(*tasks)
    duration = time.time() - t0
    total_cmds = num_clients * commands_per_client
    print(f"Processed {num_clients} clients sending {total_cmds} commands total in {duration:.4f} seconds ({total_cmds/duration:.2f} commands/sec)")

async def test_malformed_binary_frames(uri):
    print("\n--- Running Test 3: Malformed Binary Frames ---")
    async with websockets.connect(uri) as ws:
        # Scenario 1: Payload size header larger than 1MB (limit check)
        print("Sending frame with payload_size > 1MB in header (1.5MB claim)...")
        huge_size = 1500000
        frame = struct.pack("<BII", OP_AUTH_HANDSHAKE, 1001, huge_size) + b"short"
        await ws.send(frame)
        try:
            await ws.send(json.dumps({"id": "ping_post_huge", "command": "ping", "payload": {}}))
            resp = await asyncio.wait_for(ws.recv(), timeout=1.0)
            data = json.loads(resp)
            print(f"Connection remained alive after huge size header: {data}")
        except Exception as e:
            print(f"Connection closed or timed out as expected / tolerated: {e}")

    # Re-connect to test next scenarios
    async with websockets.connect(uri) as ws:
        # Scenario 2: Payload size header larger than actual data sent
        print("Sending frame with payload_size header > actual bytes sent...")
        frame = struct.pack("<BII", OP_AUTH_HANDSHAKE, 1002, 100) + b"1234567890"
        await ws.send(frame)
        
        # Scenario 3: Payload size header smaller than actual data sent
        print("Sending frame with payload_size header < actual bytes sent...")
        frame = struct.pack("<BII", OP_AUTH_HANDSHAKE, 1003, 5) + b"A" * 50
        await ws.send(frame)

        # Scenario 4: Send completely random binary data
        print("Sending completely random binary bytes...")
        await ws.send(os.urandom(256))

        # Scenario 5: Send zero-length binary frame
        print("Sending empty binary frame...")
        await ws.send(b"")

        # Verify we can still recover and ping
        print("Verifying connection health after malformed binary payloads...")
        await ws.send(json.dumps({"id": "ping_post_binary", "command": "ping", "payload": {}}))
        try:
            resp = await asyncio.wait_for(ws.recv(), timeout=1.0)
            # If the server processed the handshake in Scenario 3, it would have sent a binary echo response back.
            # We check if we got a binary frame first.
            if isinstance(resp, bytes):
                print(f"Received binary frame (likely handshake echo): {resp}")
                resp = await asyncio.wait_for(ws.recv(), timeout=1.0)
                
            data = json.loads(resp)
            assert data.get("status") == "ok", f"Expected ok, got {data}"
            print("Recovery check passed after malformed binary.")
        except asyncio.TimeoutError:
            print("FAILED: Connection did not respond after malformed binary")
            raise

async def test_immediate_disconnects(uri):
    print("\n--- Running Test 4: Immediate Disconnects Mid-Transaction ---")
    
    # Scenario 1: Connect, send partial binary frame header (5 bytes out of 9), and close TCP abruptly
    print("Abruptly disconnecting after partial binary header...")
    ws = await websockets.connect(uri)
    await ws.send(struct.pack("<BI", OP_AUTH_HANDSHAKE, 2001)) # only 5 bytes
    ws.transport.close()
    await asyncio.sleep(0.1)

    # Scenario 2: Connect, send handshake, then immediately close TCP abruptly
    print("Abruptly disconnecting immediately after handshake payload...")
    ws = await websockets.connect(uri)
    payload = b"handshake_disconnect"
    frame = struct.pack("<BII", OP_AUTH_HANDSHAKE, 2002, len(payload)) + payload
    await ws.send(frame)
    ws.transport.close()
    await asyncio.sleep(0.1)

    # Scenario 3: Connect, send rapid commands, and close TCP abruptly before reading response
    print("Abruptly disconnecting after multiple rapid commands...")
    ws = await websockets.connect(uri)
    for i in range(10):
        await ws.send(json.dumps({"id": f"disc_{i}", "command": "ping", "payload": {}}))
    ws.transport.close()
    await asyncio.sleep(0.1)
    
    print("Disconnect test cases triggered successfully.")

async def main():
    possible_paths = [
        os.path.join("target", "debug", "liva-native-core.exe"),
        os.path.join("target", "debug", "liva-native-core"),
        os.path.join("liva-native-core", "target", "debug", "liva-native-core.exe"),
        os.path.join("liva-native-core", "target", "debug", "liva-native-core"),
    ]
    server_path = None
    for p in possible_paths:
        if os.path.exists(p):
            server_path = p
            break
            
    if not server_path:
        print("Error: Compiled server binary not found. Try running cargo build first.")
        sys.exit(1)
        
    env = os.environ.copy()
    env["LIVA_DB_IN_MEMORY"] = "1"
    env["LIVA_ENCRYPTION_KEY"] = "00000000000000000000000000000000"
    env.pop("TELEGRAM_BOT_TOKEN", None)
    env["LIVA_TOKIO_WORKER_THREADS"] = "4"
    
    print(f"Starting server process: {server_path}")
    server_proc = None
    try:
        log_file = open("server_stress_test.log", "w", encoding="utf-8")
        server_proc = subprocess.Popen(
            [server_path],
            env=env,
            stdout=log_file,
            stderr=log_file
        )
        
        uri = "ws://127.0.0.1:8002/ws"
        connected = False
        print("Connecting to LIVA WebSocket server...")
        for attempt in range(50):
            try:
                if server_proc.poll() is not None:
                    raise RuntimeError("Server process exited unexpectedly.")
                async with websockets.connect(uri) as websocket:
                    connected = True
                    break
            except Exception:
                await asyncio.sleep(0.1)
                
        if not connected:
            raise RuntimeError("Could not connect to WebSocket server after 5 seconds.")
            
        print("Connected! Starting stress tests...")
        
        p_proc = psutil.Process(server_proc.pid)
        init_mem = p_proc.memory_info().rss / (1024 * 1024)
        init_handles = p_proc.num_handles() if hasattr(p_proc, 'num_handles') else "N/A"
        
        print(f"Initial server resource state: Memory: {init_mem:.2f} MB, Handles: {init_handles}")
        
        for iteration in range(1, 4):
            print(f"\n=== ITERATION {iteration} ===")
            await test_malformed_json(uri)
            await test_high_frequency_commands(uri)
            await test_malformed_binary_frames(uri)
            await test_immediate_disconnects(uri)
            
            print(f"Waiting 5 seconds for resources to settle...")
            await asyncio.sleep(5.0)
            
            iter_mem = p_proc.memory_info().rss / (1024 * 1024)
            iter_handles = p_proc.num_handles() if hasattr(p_proc, 'num_handles') else "N/A"
            print(f"Iteration {iteration} resource state: Memory: {iter_mem:.2f} MB, Handles: {iter_handles}")
            if isinstance(init_handles, int) and isinstance(iter_handles, int):
                print(f"Handle diff from start: {iter_handles - init_handles:+d}")
        
        if server_proc.poll() is not None:
            raise RuntimeError("Server process crashed during or after stress tests!")
        
        final_mem = p_proc.memory_info().rss / (1024 * 1024)
        final_handles = p_proc.num_handles() if hasattr(p_proc, 'num_handles') else "N/A"
        
        print(f"\nFinal server resource state: Memory: {final_mem:.2f} MB, Handles: {final_handles}")
        print(f"Memory diff: {final_mem - init_mem:+.2f} MB")
        if isinstance(init_handles, int) and isinstance(final_handles, int):
            print(f"Handle diff: {final_handles - init_handles:+d}")
            
        print("\nAll stress tests completed successfully!")
        
    except Exception as e:
        print(f"\nStress testing failed: {e}")
        try:
            log_file.close()
            with open("server_stress_test.log", "r", encoding="utf-8", errors="replace") as f:
                print("\n--- SERVER LOGS ---")
                lines = f.readlines()
                for line in lines[-50:]:
                    print(line, end="")
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
