import unittest
import sys
import asyncio
from unittest.mock import MagicMock, patch
import grpc

import liva_engine_pb2
from liva_native_engine import LivaNativeEngine, LivaEngineWrapper, LivaInferenceServicer

class TestVramGuardIntegration(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.lib_patcher = patch("liva_native_engine.lib")
        self.mock_lib = self.lib_patcher.start()
        self.mmap_patcher = patch("liva_native_engine.should_use_mmap")
        self.mock_should_use_mmap = self.mmap_patcher.start()

        # Mock C library calls to prevent loading actual DLL
        self.mock_should_use_mmap.return_value = True
        self.mock_lib.llama_model_load_from_file.return_value = MagicMock()
        self.mock_lib.llama_model_get_vocab.return_value = MagicMock()
        self.mock_lib.llama_vocab_eos.return_value = 2
        self.mock_lib.llama_vocab_bos.return_value = 1
        self.mock_lib.llama_context_default_params.return_value = MagicMock()
        self.mock_lib.llama_init_from_model.return_value = MagicMock()
        self.mock_lib.llama_n_ctx.return_value = 8192
        self.mock_lib.llama_sampler_chain_default_params.return_value = MagicMock()
        self.mock_lib.llama_sampler_chain_init.return_value = MagicMock()

        self.engine = LivaNativeEngine(
            model_path="dummy_path.gguf",
            n_threads=1,
            n_threads_batch=1
        )
        
        # Populate speculative and dedicated model/context attributes to assert they are freed
        self.engine.draft_model = MagicMock()
        self.engine.draft_ctx = MagicMock()
        self.engine.draft_sampler = MagicMock()
        self.engine.embed_ctx = MagicMock()
        self.engine.embed_memory = MagicMock()
        self.engine._cached_tokens = [1, 2, 3]

        with patch("liva_native_engine.EngineFactory.create_engine", return_value=self.engine):
            self.wrapper = LivaEngineWrapper(
                initial_backend="llama.cpp",
                model_path="dummy_path.gguf"
            )
            
        self.servicer = LivaInferenceServicer(self.wrapper)

    def tearDown(self):
        self.lib_patcher.stop()
        self.mmap_patcher.stop()

    @patch("sys.platform", "darwin")
    @patch("asyncio.sleep")
    @patch("subprocess.check_output")
    async def test_vram_guard_integration_flow(self, mock_check_output, mock_sleep):
        # We need two iterations:
        # Iteration 1: Heavy app detected (Xcode/Blender) -> shutdown is called, is_yielded becomes True
        # Iteration 2: No heavy app -> sys.exit(0) is called
        
        mock_check_output.side_effect = [
            "COMM\n/Applications/Xcode.app/Contents/MacOS/Xcode\n",
            "COMM\n/usr/libexec/logd\n"
        ]
        
        # We mock sleep to do nothing so the loop proceeds immediately
        async def mock_async_sleep(*args, **kwargs):
            pass
        mock_sleep.side_effect = mock_async_sleep
        
        # Spy/Mock engine.shutdown to make sure it executes the real shutdown but also lets us verify
        original_shutdown = self.engine.shutdown
        shutdown_called = False
        def spy_shutdown(*args, **kwargs):
            nonlocal shutdown_called
            shutdown_called = True
            original_shutdown(*args, **kwargs)
            
        self.engine.shutdown = spy_shutdown

        # Run the vram_guard_loop and catch SystemExit(0) when the heavy app exits
        system_exit_raised = False
        try:
            await self.wrapper.vram_guard_loop()
        except SystemExit as e:
            self.assertEqual(e.code, 0)
            system_exit_raised = True
            
        # ASSERTIONS:
        
        # e) Once heavy app exits, the daemon terminates with exit code 0
        self.assertTrue(system_exit_raised, "vram_guard_loop should raise SystemExit(0) when heavy app exits")
        
        # a) self.shutdown() is called and unloads all models/contexts.
        self.assertTrue(shutdown_called, "engine.shutdown should have been called")
        
        # b) Pointer attributes are set to None and VRAM is completely freed.
        self.assertIsNone(self.engine.model)
        self.assertIsNone(self.engine.ctx)
        self.assertIsNone(self.engine.sampler)
        self.assertIsNone(self.engine.draft_model)
        self.assertIsNone(self.engine.draft_ctx)
        self.assertIsNone(self.engine.draft_sampler)
        self.assertIsNone(self.engine.embed_ctx)
        self.assertIsNone(self.engine.embed_memory)
        self.assertIsNone(self.engine._cached_tokens)
        
        # c) HealthCheck returns alive=False.
        health_ctx = MagicMock()
        health_resp = await self.servicer.HealthCheck(liva_engine_pb2.HealthRequest(), health_ctx)
        self.assertFalse(health_resp.alive)
        
        # d) Tokenization queries reject with UNAVAILABLE.
        class MockGrpcContext:
            def __init__(self):
                self.code = None
                self.details = None

            def set_code(self, code):
                self.code = code

            def set_details(self, details):
                self.details = details

        # Unary Chat completion check
        chat_req = liva_engine_pb2.ChatCompletionRequest(
            messages=[liva_engine_pb2.ChatMessage(role="user", content="hello")]
        )
        chat_ctx = MockGrpcContext()
        chat_resp = await self.servicer.Chat(chat_req, chat_ctx)
        self.assertEqual(chat_ctx.code, grpc.StatusCode.UNAVAILABLE)
        self.assertEqual(chat_ctx.details, "VRAM yielded")
        self.assertIsInstance(chat_resp, liva_engine_pb2.ChatCompletionResponse)
        self.assertEqual(len(chat_resp.choices), 0)
        
        # Streaming Chat completion check
        stream_ctx = MockGrpcContext()
        stream_chunks = []
        async for chunk in self.servicer.StreamChat(chat_req, stream_ctx):
            stream_chunks.append(chunk)
        self.assertEqual(stream_ctx.code, grpc.StatusCode.UNAVAILABLE)
        self.assertEqual(stream_ctx.details, "VRAM yielded")
        self.assertEqual(len(stream_chunks), 0)

if __name__ == "__main__":
    unittest.main()
