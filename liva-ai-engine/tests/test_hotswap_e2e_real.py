import unittest
import asyncio
import subprocess
import os
import sys
import time
import grpc

# Ensure the root directory is in sys.path to import liva_engine_pb2
sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import liva_engine_pb2
import liva_engine_pb2_grpc

# Force utf-8 for stdout
sys.stdout.reconfigure(encoding='utf-8')

class TestHotSwapE2EReal(unittest.IsolatedAsyncioTestCase):
    daemon_process = None

    async def asyncSetUp(self):
        # We need to start the liva_native_engine.py daemon in the background
        # using the correct env variables.
        # ROUTER_MODEL_NAME=gemma-4-E4B-it-Q4_K_M.gguf
        # EXPERT_MODEL_NAME=gemma-4-12B-it-qat-UD-Q4_K_XL.gguf
        
        env = os.environ.copy()
        env["ROUTER_MODEL_NAME"] = "gemma-4-E4B-it-Q4_K_M.gguf"
        env["EXPERT_MODEL_NAME"] = "gemma-4-12B-it-qat-UD-Q4_K_XL.gguf"
        env["AI_MODELS_DIR"] = "/Users/duongnad/AI_Models"
        env["LIVA_DISABLE_MEMORY_PRESSURE_CHECK"] = "1"
        
        
        # Ensure the subprocess can import grpc, psutil etc. from venv
        project_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        venv_site = os.path.join(project_dir, "venv/lib/python3.11/site-packages")
        env["PYTHONPATH"] = venv_site + os.pathsep + env.get("PYTHONPATH", "")
        
        # Add native_lib to DYLD_LIBRARY_PATH so ctypes can load libllama.dylib
        native_lib_dir = os.path.join(project_dir, "native_lib")
        env["DYLD_LIBRARY_PATH"] = native_lib_dir + os.pathsep + env.get("DYLD_LIBRARY_PATH", "")
        env["LD_LIBRARY_PATH"] = native_lib_dir + os.pathsep + env.get("LD_LIBRARY_PATH", "")
        
        engine_script = os.path.join(project_dir, "liva_native_engine.py")
        python_interpreter = "/Users/duongnad/.pyenv/shims/python3"
        
        print(f"\n[E2E] Starting native engine daemon: {python_interpreter} {engine_script}", flush=True)
        # Start daemon process in background and redirect output to a file to prevent buffer deadlocks
        self.daemon_log = open("liva_native_engine_daemon.log", "w", encoding="utf-8")
        self.daemon_process = subprocess.Popen(
            [python_interpreter, engine_script],
            env=env,
            stdout=self.daemon_log,
            stderr=self.daemon_log,
            text=True
        )
        
        # Create gRPC channel and stub
        self.channel = grpc.aio.insecure_channel('127.0.0.1:8100')
        self.stub = liva_engine_pb2_grpc.LivaInferenceServiceStub(self.channel)

    async def asyncTearDown(self):
        # Clean up and kill the daemon process
        await self.channel.close()
        if self.daemon_process:
            print("\n[E2E] Terminating native engine daemon...", flush=True)
            self.daemon_process.terminate()
            try:
                # wait for termination
                self.daemon_process.wait(timeout=8)
            except subprocess.TimeoutExpired:
                print("[E2E] Daemon did not terminate in time. Killing...", flush=True)
                self.daemon_process.kill()
                self.daemon_process.wait()
        
        if hasattr(self, "daemon_log") and self.daemon_log:
            self.daemon_log.close()
            if os.path.exists("liva_native_engine_daemon.log"):
                with open("liva_native_engine_daemon.log", "r", encoding="utf-8") as f:
                    print(f"[E2E] Daemon Log:\n{f.read()}", flush=True)

    async def test_hotswap_e2e(self):
        # 1. Verify health check first (waiting for startup)
        print("[E2E] Waiting for daemon health check to pass...", flush=True)
        health_ok = False
        health_res = None
        for i in range(45):
            try:
                health_res = await self.stub.HealthCheck(liva_engine_pb2.HealthRequest(), timeout=2.0)
                if health_res.alive:
                    print(f"[E2E] Daemon is alive! Model name: {health_res.model_name}", flush=True)
                    health_ok = True
                    break
            except Exception as e:
                # Server is starting up
                pass
            await asyncio.sleep(1.0)
            
        self.assertTrue(health_ok, "Daemon health check failed to respond alive=True within 45 seconds")
        self.assertIsNotNone(health_res)
        print(f"[E2E] Initial HealthCheck Response:\n{health_res}", flush=True)
        
        # 2. Call SwapModel to load the Expert model: gemma-4-12B-it-qat-UD-Q4_K_XL.gguf
        expert_model_path = "/Users/duongnad/AI_Models/gemma-4-12B-it-qat-UD-Q4_K_XL.gguf"
        print(f"\n[E2E] Swapping model to Expert: {expert_model_path}", flush=True)
        swap_req = liva_engine_pb2.SwapModelRequest(
            model_path=expert_model_path,
            n_ctx=8192,
            n_gpu_layers=-1,
            backend="llama.cpp"
        )
        
        start_time = time.perf_counter()
        swap_res = await self.stub.SwapModel(swap_req)
        end_time = time.perf_counter()
        duration = end_time - start_time
        print(f"[E2E] Swap to Expert completed in {duration:.4f}s.", flush=True)
        print(f"[E2E] Swap response: success={swap_res.success}, loaded_model={swap_res.loaded_model}, error_message={swap_res.error_message}, swap_duration_ms={swap_res.swap_duration_ms}", flush=True)
        
        self.assertTrue(swap_res.success, f"Failed to swap to Expert model: {swap_res.error_message}")
        
        # 3. Call Chat or StreamChat to perform a simple inference request on the Expert model
        print("\n[E2E] Performing chat inference on Expert model...", flush=True)
        chat_req = liva_engine_pb2.ChatCompletionRequest(
            model="liva-native",
            messages=[
                liva_engine_pb2.ChatMessage(role="user", content="Explain briefly what is dynamic hot-swapping in 1 sentence.")
            ],
            max_tokens=50
        )
        
        chat_res = await self.stub.Chat(chat_req)
        self.assertGreater(len(chat_res.choices), 0, "No chat completion choices returned from Expert model")
        expert_reply = chat_res.choices[0].message.content
        print(f"[E2E] Expert model reply: {expert_reply.strip()}", flush=True)
        
        # 4. Call SwapModel to swap back to the Router model: gemma-4-E4B-it-Q4_K_M.gguf
        router_model_path = "/Users/duongnad/AI_Models/gemma-4-E4B-it-Q4_K_M.gguf"
        print(f"\n[E2E] Swapping model back to Router: {router_model_path}", flush=True)
        swap_back_req = liva_engine_pb2.SwapModelRequest(
            model_path=router_model_path,
            n_ctx=8192,
            n_gpu_layers=-1,
            backend="llama.cpp"
        )
        
        start_time_back = time.perf_counter()
        swap_back_res = await self.stub.SwapModel(swap_back_req)
        end_time_back = time.perf_counter()
        duration_back = end_time_back - start_time_back
        print(f"[E2E] Swap back to Router completed in {duration_back:.4f}s.", flush=True)
        print(f"[E2E] Swap response: success={swap_back_res.success}, loaded_model={swap_back_res.loaded_model}, error_message={swap_back_res.error_message}, swap_duration_ms={swap_back_res.swap_duration_ms}", flush=True)
        
        self.assertTrue(swap_back_res.success, f"Failed to swap back to Router model: {swap_back_res.error_message}")
        
        # 5. Performs a simple inference on the Router model
        print("\n[E2E] Performing chat inference on Router model...", flush=True)
        chat_req_router = liva_engine_pb2.ChatCompletionRequest(
            model="liva-native",
            messages=[
                liva_engine_pb2.ChatMessage(role="user", content="Hello, say hello in 1 word.")
            ],
            max_tokens=20
        )
        
        chat_res_router = await self.stub.Chat(chat_req_router)
        self.assertGreater(len(chat_res_router.choices), 0, "No chat completion choices returned from Router model")
        router_reply = chat_res_router.choices[0].message.content
        print(f"[E2E] Router model reply: {router_reply.strip()}", flush=True)

if __name__ == '__main__':
    unittest.main()
