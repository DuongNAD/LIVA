import unittest
import sys
import asyncio
from unittest.mock import MagicMock, patch
from liva_native_engine import LivaNativeEngine, LivaInferenceServicer, HAS_GET_MEMORY

class TestNativeEngineFixes(unittest.IsolatedAsyncioTestCase):
    @patch("liva_native_engine.lib")
    @patch("liva_native_engine.should_use_mmap")
    def setUp(self, mock_should_use_mmap, mock_lib):
        # Mock C library calls to prevent loading actual DLL
        mock_should_use_mmap.return_value = True
        mock_lib.llama_model_load_from_file.return_value = MagicMock()
        mock_lib.llama_model_get_vocab.return_value = MagicMock()
        mock_lib.llama_vocab_eos.return_value = 2
        mock_lib.llama_vocab_bos.return_value = 1
        mock_lib.llama_context_default_params.return_value = MagicMock()
        mock_lib.llama_init_from_model.return_value = MagicMock()
        mock_lib.llama_n_ctx.return_value = 8192
        mock_lib.llama_sampler_chain_default_params.return_value = MagicMock()
        mock_lib.llama_sampler_chain_init.return_value = MagicMock()

        with patch("sys.platform", "linux"):
            self.engine = LivaNativeEngine(
                model_path="dummy_path.gguf",
                n_threads=1,
                n_threads_batch=1
            )
        self.engine.shutdown = MagicMock()

    @patch("liva_native_engine.lib")
    def test_context_recreation_on_init_and_failure(self, mock_lib):
        # Verify that embed_ctx_params was saved during setUp init
        self.assertIsNotNone(self.engine.embed_ctx_params)

        # Mock llama_init_from_model to return None (simulating recreation failure)
        mock_lib.llama_init_from_model.return_value = None
        
        # Test recreation check top of _get_embeddings_batch_unsafe
        # Let's temporarily set self.engine.embed_ctx to None to trigger recreation
        self.engine.embed_ctx = None
        self.engine.embed_memory = MagicMock()
        
        # Call the internal method which will trigger recreation
        import numpy as np
        try:
            self.engine._get_embeddings_batch_unsafe(["hello"], 128, np)
        except Exception:
            pass

        # Since it returned None, self.engine.embed_ctx and self.engine.embed_memory must be None
        self.assertIsNone(self.engine.embed_ctx)
        self.assertIsNone(self.engine.embed_memory)

        # Mock llama_init_from_model to return a valid context (simulating recreation success)
        new_mock_ctx = MagicMock()
        mock_lib.llama_init_from_model.return_value = new_mock_ctx
        
        # Reset embed_ctx to None again to force recreation
        self.engine.embed_ctx = None
        try:
            self.engine._get_embeddings_batch_unsafe(["hello"], 128, np)
        except Exception:
            pass

        self.assertEqual(self.engine.embed_ctx, new_mock_ctx)

    @patch("liva_native_engine.lib")
    def test_dynamic_context_size_guard(self, mock_lib):
        # Set llama_n_ctx to return 10
        mock_lib.llama_n_ctx.return_value = 10
        
        # Mock tokenize to return a list of 20 tokens
        self.engine.tokenize = MagicMock(return_value=list(range(20)))
        
        # We want to check truncation inside _get_embeddings_batch_unsafe (single text path)
        mock_lib.llama_decode.return_value = 0
        mock_lib.llama_get_embeddings.return_value = MagicMock()
        
        mock_batch = MagicMock()
        mock_lib.llama_batch_init.return_value = mock_batch
        
        import numpy as np
        # Mock numpy as_array
        with patch("numpy.ctypeslib.as_array") as mock_as_array:
            mock_as_array.return_value = np.zeros(128)
            self.engine._get_embeddings_batch_unsafe(["hello"], 128, np)
            
            # Since active_n_ctx = 10, the token limit should be 10 - 4 = 6.
            # Thus, the mock tokenize return should be truncated to 6 tokens.
            self.engine.tokenize.assert_called_once_with("hello", add_special=True)
            # Assert that mock_batch.n_tokens equals 6
            self.assertEqual(mock_batch.n_tokens, 6)

    @patch("liva_native_engine.lib")
    def test_sequence_allocation_simplified(self, mock_lib):
        # Call get_embeddings_batch to trigger llama_batch_init
        import numpy as np
        mock_lib.llama_decode.return_value = 0
        mock_lib.llama_get_embeddings.return_value = MagicMock()
        
        with patch("numpy.ctypeslib.as_array") as mock_as_array:
            mock_as_array.return_value = np.zeros(128)
            try:
                self.engine._get_embeddings_batch_unsafe(["hello", "world"], 128, np)
            except Exception:
                pass
            
            # Assert that llama_batch_init was called with third parameter = 1 (n_seq_max)
            mock_lib.llama_batch_init.assert_called_with(self.engine.n_batch, 0, 1)

    @patch("liva_native_engine.lib")
    async def test_stream_chat_queue_draining(self, mock_lib):
        servicer = LivaInferenceServicer(self.engine)
        
        # Mock request structure matching protobuf request
        class MockMessage:
            def __init__(self, role, content):
                self.role = role
                self.content = content
        
        class MockRequest:
            request_id = "test_req"
            messages = [MockMessage("user", "Hello!")]
            max_tokens = 50
            
        request = MockRequest()
        
        # Mock engine.tokenize and engine.generate_stream
        self.engine.tokenize = MagicMock(return_value=[1, 2, 3])
        
        # Helper to simulate generator output chunks
        def mock_generate_stream(tokens, max_tokens):
            yield "chunk1"
            yield "chunk2"
            yield "chunk3"
            
        self.engine.generate_stream = MagicMock(side_effect=mock_generate_stream)
        
        # Execute StreamChat
        async for chunk in servicer.StreamChat(request, None):
            self.assertIsNotNone(chunk)
            # Ensure the chunks are gathered correctly
            # (StreamChat returns ChatCompletionResponse/StreamChunk objects, we can verify it doesn't hang or fail)
            break

    @patch("liva_native_engine.lib")
    async def test_stream_chat_stop_trigger(self, mock_lib):
        servicer = LivaInferenceServicer(self.engine)
        
        class MockMessage:
            def __init__(self, role, content):
                self.role = role
                self.content = content
        
        class MockRequest:
            request_id = "test_req"
            messages = [MockMessage("user", "Hello!")]
            max_tokens = 50
            
        request = MockRequest()
        self.engine.tokenize = MagicMock(return_value=[1, 2, 3])
        
        def mock_generate_stream(tokens, max_tokens):
            yield "hello"
            yield " world"
            yield "<end"
            yield "_of_turn>"
            
        self.engine.generate_stream = MagicMock(side_effect=mock_generate_stream)
        
        chunks = []
        async for chunk in servicer.StreamChat(request, None):
            chunks.append(chunk)
            
        content_yielded = ""
        finish_reason_stop = False
        for c in chunks:
            for choice in c.choices:
                if choice.delta.content:
                    content_yielded += choice.delta.content
                if choice.finish_reason == "stop":
                    finish_reason_stop = True
                    
        self.assertEqual(content_yielded, "hello world")
        self.assertTrue(finish_reason_stop)

    @patch("liva_native_engine.lib")
    async def test_stream_chat_natural_end_with_partial_match(self, mock_lib):
        servicer = LivaInferenceServicer(self.engine)
        
        class MockMessage:
            def __init__(self, role, content):
                self.role = role
                self.content = content
        
        class MockRequest:
            request_id = "test_req"
            messages = [MockMessage("user", "Hello!")]
            max_tokens = 50
            
        request = MockRequest()
        self.engine.tokenize = MagicMock(return_value=[1, 2, 3])
        
        def mock_generate_stream(tokens, max_tokens):
            yield "hello"
            yield "<start_of_"
            
        self.engine.generate_stream = MagicMock(side_effect=mock_generate_stream)
        
        chunks = []
        async for chunk in servicer.StreamChat(request, None):
            chunks.append(chunk)
            
        content_yielded = ""
        finish_reason_stop = False
        for c in chunks:
            for choice in c.choices:
                if choice.delta.content:
                    content_yielded += choice.delta.content
                if choice.finish_reason == "stop":
                    finish_reason_stop = True
                    
        self.assertEqual(content_yielded, "hello<start_of_")
        self.assertTrue(finish_reason_stop)

    @patch("liva_native_engine.lib")
    def test_embedding_context_recreation_null_guard(self, mock_lib):
        import numpy as np
        
        # Save original params
        self.engine.embed_ctx_params = MagicMock()
        
        # Set embed_ctx to None
        self.engine.embed_ctx = None
        
        # Scenario A: Init from model returns None
        mock_lib.llama_init_from_model.return_value = None
        try:
            self.engine._get_embeddings_batch_unsafe(["hello"], 128, np)
        except Exception:
            pass
            
        # Scenario B: Init from model returns a valid context
        mock_lib.llama_init_from_model.return_value = MagicMock()
        try:
            self.engine._get_embeddings_batch_unsafe(["hello"], 128, np)
        except Exception:
            pass
            
        # Verify that llama_free was never called with None
        for call in mock_lib.llama_free.mock_calls:
            args = call[1]
            if args:
                self.assertIsNotNone(args[0], "llama_free was called with NULL pointer (None)")

    @patch("liva_native_engine.lib")
    def test_generate_stream_generator_exit_clears_cached_tokens(self, mock_lib):
        # Set engine cached tokens to some initial value
        self.engine._cached_tokens = [1, 2, 3]
        
        # Mock _generate_stream_unsafe to raise GeneratorExit
        def mock_unsafe(*args, **kwargs):
            raise GeneratorExit("cancelled")
            
        self.engine._generate_stream_unsafe = MagicMock(side_effect=mock_unsafe)
        
        # Call generate_stream and consume it to trigger the generator execution
        gen = self.engine.generate_stream([1, 2, 3], max_tokens=10)
        with self.assertRaises(GeneratorExit):
            list(gen)
            
        # Verify cached tokens were cleared
        self.assertIsNone(self.engine._cached_tokens)

    @patch("liva_native_engine.lib")
    def test_get_embeddings_batch_unsafe_no_attribute_embed_ctx(self, mock_lib):
        import numpy as np
        
        # Save original params and make sure we have params
        self.engine.embed_ctx_params = MagicMock()
        
        # Simulate embed_ctx not existing on the object (AttributeError scenario)
        if hasattr(self.engine, "embed_ctx"):
            delattr(self.engine, "embed_ctx")
            
        # Mock required lib calls
        mock_lib.llama_n_ctx.return_value = 8192
        mock_lib.llama_init_from_model.return_value = MagicMock()
        mock_lib.llama_decode.return_value = 0
        mock_lib.llama_get_embeddings.return_value = MagicMock()
        
        # Mock tokenize to avoid calling mocked lib.llama_tokenize
        self.engine.tokenize = MagicMock(return_value=[1, 2, 3])
        
        with patch("numpy.ctypeslib.as_array") as mock_as_array:
            mock_as_array.return_value = np.zeros(128)
            # This should not raise AttributeError
            self.engine._get_embeddings_batch_unsafe(["hello"], 128, np)
            
        # Verify that embed_ctx was successfully recreated and assigned
        self.assertIsNotNone(self.engine.embed_ctx)

    @patch("liva_native_engine.lib")
    @patch("sys.platform", "darwin")
    def test_init_draft_model_macos_thread_counts_explicit_one(self, mock_lib):
        # Setup env variables to enable speculative
        with patch.dict("os.environ", {"LIVA_ENABLE_SPECULATIVE": "true", "LIVA_DRAFT_MODEL_NAME": "draft.gguf"}):
            with patch("os.path.exists", return_value=True):
                mock_lib.llama_model_load_from_file.return_value = MagicMock()
                mock_lib.llama_context_default_params.return_value = MagicMock()
                
                # We want to capture the parameters passed to llama_init_from_model
                mock_lib.llama_init_from_model.return_value = MagicMock()
                
                # Call _init_draft_model with n_threads = 1
                self.engine._init_draft_model(n_ctx=512, n_gpu_layers=-1, n_batch=512, n_threads=1, flash_attn=False)
                
                # Check that llama_init_from_model was called with the context params having n_threads = 1
                args, kwargs = mock_lib.llama_init_from_model.call_args
                ctx_params = args[1]
                self.assertEqual(ctx_params.n_threads, 1)
                self.assertEqual(ctx_params.n_threads_batch, 1)

    @patch("liva_native_engine.lib")
    def test_mutex_selection_and_fallback(self, mock_lib):
        # Mock the locks
        self.engine._embed_mutex = MagicMock()
        self.engine._engine_mutex = MagicMock()
        
        # Mock required methods of engine and lib
        self.engine.get_embedding_dim = MagicMock(return_value=128)
        self.engine._get_embeddings_batch_unsafe = MagicMock(return_value=[[0.1] * 128])
        
        import numpy as np
        
        # Case A: embed_ctx is present (not None) -> should use embed_mutex first
        self.engine.embed_ctx = MagicMock()
        res = self.engine.get_embeddings_batch(["test"])
        
        self.engine._embed_mutex.__enter__.assert_called_once()
        self.engine._engine_mutex.__enter__.assert_not_called()
        self.engine._get_embeddings_batch_unsafe.assert_called_once_with(["test"], 128, np, use_dedicated=True)
        
        # Reset mocks
        self.engine._embed_mutex.reset_mock()
        self.engine._engine_mutex.reset_mock()
        self.engine._get_embeddings_batch_unsafe.reset_mock()
        
        # Case B: embed_ctx is None -> should fallback to engine_mutex
        self.engine.embed_ctx = None
        res = self.engine.get_embeddings_batch(["test"])
        
        self.engine._embed_mutex.__enter__.assert_not_called()
        self.engine._engine_mutex.__enter__.assert_called_once()
        self.engine._get_embeddings_batch_unsafe.assert_called_once_with(["test"], 128, np, use_dedicated=False)

        # Reset mocks
        self.engine._embed_mutex.reset_mock()
        self.engine._engine_mutex.reset_mock()
        self.engine._get_embeddings_batch_unsafe.reset_mock()

        # Case C: Double-check lock fallback (embed_ctx was non-None but becomes None inside the block)
        self.engine.embed_ctx = MagicMock()
        
        def side_effect(*args, **kwargs):
            self.engine.embed_ctx = None
            return MagicMock()
            
        self.engine._embed_mutex.__enter__.side_effect = side_effect
        
        res = self.engine.get_embeddings_batch(["test"])
        self.engine._embed_mutex.__enter__.assert_called_once()
        self.engine._engine_mutex.__enter__.assert_called_once()
        self.engine._get_embeddings_batch_unsafe.assert_called_once_with(["test"], 128, np, use_dedicated=False)

    @patch("liva_native_engine.lib")
    def test_memory_leak_on_recreation_failure(self, mock_lib):
        import numpy as np
        
        # 1. Single text path decode failure scenario
        mock_lib.llama_n_ctx.return_value = 1024
        mock_lib.llama_decode.return_value = -1  # Simulation of decode failure
        mock_lib.llama_init_from_model.return_value = None  # Simulation of recreation failure
        
        self.engine.embed_ctx_params = MagicMock()
        old_ctx_mock = MagicMock()
        self.engine.embed_ctx = old_ctx_mock
        
        # Mock tokenize
        self.engine.tokenize = MagicMock(return_value=[1, 2, 3])
        
        with self.assertRaises(RuntimeError):
            self.engine._get_embeddings_batch_unsafe(["test_single"], 128, np, use_dedicated=True)
            
        # Verify llama_free was called with the old_ctx_mock
        mock_lib.llama_free.assert_any_call(old_ctx_mock)
        self.assertIsNone(self.engine.embed_ctx)
        
        # Reset mocks
        mock_lib.llama_free.reset_mock()
        mock_lib.llama_decode.reset_mock()
        
        # 2. Multi-text path decode failure scenario
        mock_lib.llama_decode.return_value = -1  # Simulation of decode failure
        mock_lib.llama_init_from_model.return_value = None  # Simulation of recreation failure
        
        old_ctx_mock_multi = MagicMock()
        self.engine.embed_ctx = old_ctx_mock_multi
        
        # Tokenize returns a list
        self.engine.tokenize = MagicMock(return_value=[1, 2, 3])
        
        with self.assertRaises(RuntimeError):
            # Pass two texts to trigger multi-text path
            self.engine._get_embeddings_batch_unsafe(["test_multi_1", "test_multi_2"], 128, np, use_dedicated=True)
            
        # Verify llama_free was called with old_ctx_mock_multi
        mock_lib.llama_free.assert_any_call(old_ctx_mock_multi)
        self.assertIsNone(self.engine.embed_ctx)

    @patch("liva_native_engine.lib")
    def test_recreate_mutex_acquired_on_context_recreation(self, mock_lib):
        # Setup: embed_ctx is None, embed_ctx_params is not None
        self.engine.embed_ctx = None
        self.engine.embed_ctx_params = MagicMock()
        mock_lib.llama_init_from_model.return_value = MagicMock()
        mock_lib.llama_n_ctx.return_value = 8192
        
        # Spy on the recreation lock
        self.engine._recreate_mutex = MagicMock(wraps=self.engine._recreate_mutex)
        
        import numpy as np
        self.engine.tokenize = MagicMock(return_value=[1, 2, 3])
        mock_lib.llama_decode.return_value = 0
        mock_lib.llama_get_embeddings.return_value = MagicMock()
        
        with patch("numpy.ctypeslib.as_array") as mock_as_array:
            mock_as_array.return_value = np.zeros(128)
            self.engine._get_embeddings_batch_unsafe(["hello"], 128, np, use_dedicated=True)
            
        # Assert that _recreate_mutex was acquired
        self.engine._recreate_mutex.__enter__.assert_called()

    @patch("liva_native_engine.lib")
    def test_dedicated_fallback_routing_raises_error(self, mock_lib):
        # Setup: use_dedicated=True, but embed_ctx is None (even after recreation fails)
        self.engine.embed_ctx = None
        self.engine.embed_ctx_params = MagicMock()
        mock_lib.llama_init_from_model.return_value = None
        
        import numpy as np
        with self.assertRaises(RuntimeError) as context:
            self.engine._get_embeddings_batch_unsafe(["hello"], 128, np, use_dedicated=True)
            
        self.assertIn("Dedicated embedding context is None under dedicated lock", str(context.exception))

    @patch("liva_native_engine.lib")
    def test_decode_failure_recreation_under_recreate_mutex(self, mock_lib):
        import numpy as np
        
        # Setup: decode failure
        mock_lib.llama_n_ctx.return_value = 1024
        mock_lib.llama_decode.return_value = -1  # Failure
        
        self.engine.embed_ctx_params = MagicMock()
        old_ctx = MagicMock()
        self.engine.embed_ctx = old_ctx
        
        self.engine._recreate_mutex = MagicMock(wraps=self.engine._recreate_mutex)
        self.engine.tokenize = MagicMock(return_value=[1, 2, 3])
        
        # Scenario A: Single text path
        with self.assertRaises(RuntimeError):
            self.engine._get_embeddings_batch_unsafe(["hello"], 128, np, use_dedicated=True)
        self.engine._recreate_mutex.__enter__.assert_called()
        
        # Reset mock
        self.engine._recreate_mutex.reset_mock()
        self.engine.embed_ctx = old_ctx
        
        # Scenario B: Multi-text path
        with self.assertRaises(RuntimeError):
            self.engine._get_embeddings_batch_unsafe(["hello", "world"], 128, np, use_dedicated=True)
        self.engine._recreate_mutex.__enter__.assert_called()

    @patch("liva_native_engine.lib")
    @patch("asyncio.sleep", new_callable=MagicMock)
    @patch("subprocess.check_output")
    async def test_vram_guard_shutdown_thread_safety(self, mock_check_output, mock_sleep, mock_lib):
        # Mock sys.platform to be win32 for the sake of the test
        with patch("sys.platform", "win32"):
            # Set up the run loop to check heavy apps once and exit loop
            mock_check_output.return_value = '"blender.exe"\n'
            
            # Setup a helper to raise an exception after the first loop iteration
            # to break the infinite while loop in vram_guard_loop
            call_count = 0
            async def mock_sleep_side_effect(delay):
                nonlocal call_count
                call_count += 1
                if call_count > 1:
                    raise asyncio.CancelledError()
                
            mock_sleep.side_effect = mock_sleep_side_effect
            
            self.engine._engine_mutex = MagicMock()
            self.engine._embed_mutex = MagicMock()
            
            # Call vram_guard_loop
            try:
                await self.engine.vram_guard_loop()
            except asyncio.CancelledError:
                pass
                
            # Verify self.shutdown was called and mutexes were acquired
            self.engine._engine_mutex.__enter__.assert_called()
            self.engine._embed_mutex.__enter__.assert_called()
            self.engine.shutdown.assert_called_once()

    @patch("liva_native_engine.lib")
    def test_negative_limit_clamping(self, mock_lib):
        # Set active_n_ctx to 2 (so active_n_ctx - 4 = -2)
        mock_lib.llama_n_ctx.return_value = 2
        mock_lib.llama_decode.return_value = 0
        mock_lib.llama_get_embeddings.return_value = MagicMock()
        
        self.engine.tokenize = MagicMock(return_value=[1, 2, 3])
        mock_batch = MagicMock()
        mock_lib.llama_batch_init.return_value = mock_batch
        
        import numpy as np
        with patch("numpy.ctypeslib.as_array") as mock_as_array:
            mock_as_array.return_value = np.zeros(128)
            
            # Single-text path: should clamp limit to 0
            self.engine._get_embeddings_batch_unsafe(["hello"], 128, np)
            self.assertEqual(mock_batch.n_tokens, 0)
            
            # Multi-text path: should clamp limit to 0
            mock_batch.reset_mock()
            self.engine._get_embeddings_batch_unsafe(["hello", "world"], 128, np)
            self.assertEqual(mock_batch.n_tokens, 0)

    @patch("liva_native_engine.lib")
    def test_generate_stream_raises_runtime_error_if_dead_or_no_ctx(self, mock_lib):
        # Case A: self.ctx is None under lock
        self.engine.ctx = None
        self.engine._alive = True
        gen = self.engine.generate_stream([1, 2, 3])
        with self.assertRaises(RuntimeError) as context:
            next(gen)
        self.assertIn("Engine is not alive — cannot generate", str(context.exception))

        # Case B: self._alive is False under lock
        self.engine.ctx = MagicMock()
        self.engine._alive = False
        gen = self.engine.generate_stream([1, 2, 3])
        with self.assertRaises(RuntimeError) as context:
            next(gen)
        self.assertIn("Engine is not alive — cannot generate", str(context.exception))

    @patch("liva_native_engine.lib")
    def test_get_embeddings_batch_fallback_raises_runtime_error_if_dead_or_no_ctx(self, mock_lib):
        self.engine.embed_ctx = None
        self.engine.get_embedding_dim = MagicMock(return_value=128)

        # Case A: self.ctx is None
        self.engine.ctx = None
        self.engine._alive = True
        with self.assertRaises(RuntimeError) as context:
            self.engine.get_embeddings_batch(["hello"])
        self.assertIn("Engine is not alive — cannot fall back to shared embedding context", str(context.exception))

        # Case B: self._alive is False
        self.engine.ctx = MagicMock()
        self.engine._alive = False
        with self.assertRaises(RuntimeError) as context:
            self.engine.get_embeddings_batch(["hello"])
        self.assertIn("Engine is not alive — cannot embed", str(context.exception))

    @patch("liva_native_engine.lib")
    def test_get_embeddings_batch_unsafe_raises_if_shared_ctx_is_none(self, mock_lib):
        import numpy as np
        self.engine.embed_ctx = None
        self.engine.ctx = None
        with self.assertRaises(RuntimeError) as context:
            self.engine._get_embeddings_batch_unsafe(["hello"], 128, np, use_dedicated=False)
        self.assertIn("Shared context (self.ctx) is None — cannot generate embeddings", str(context.exception))

    @patch("liva_native_engine.lib")
    async def test_stream_chat_tokenization_inside_lock(self, mock_lib):
        servicer = LivaInferenceServicer(self.engine)
        
        class MockMessage:
            def __init__(self, role, content):
                self.role = role
                self.content = content
        
        class MockRequest:
            request_id = "test_req"
            messages = [MockMessage("user", "Hello!")]
            max_tokens = 50
            
        request = MockRequest()
        
        lock_was_held = False
        def mock_tokenize(prompt_text, add_special=True):
            nonlocal lock_was_held
            lock_was_held = servicer.engine_lock.locked()
            return [1, 2, 3]
            
        self.engine.tokenize = MagicMock(side_effect=mock_tokenize)
        self.engine.generate_stream = MagicMock(return_value=(x for x in ["chunk"]))
        
        async for chunk in servicer.StreamChat(request, None):
            break
            
        self.assertTrue(lock_was_held, "engine_lock should be held during tokenization")

    @patch("liva_native_engine.lib")
    async def test_chat_tokenization_inside_lock(self, mock_lib):
        servicer = LivaInferenceServicer(self.engine)
        
        class MockMessage:
            def __init__(self, role, content):
                self.role = role
                self.content = content
        
        class MockRequest:
            request_id = "test_req"
            messages = [MockMessage("user", "Hello!")]
            max_tokens = 50
            
        request = MockRequest()
        
        lock_was_held = False
        def mock_tokenize(prompt_text, add_special=True):
            nonlocal lock_was_held
            lock_was_held = servicer.engine_lock.locked()
            return [1, 2, 3]
            
        self.engine.tokenize = MagicMock(side_effect=mock_tokenize)
        self.engine.generate = MagicMock(return_value="response")
        
        await servicer.Chat(request, None)
        self.assertTrue(lock_was_held, "engine_lock should be held during tokenization in Chat")

    @patch("liva_native_engine.lib")
    async def test_embed_dimension_query_inside_lock(self, mock_lib):
        servicer = LivaInferenceServicer(self.engine)
        
        class MockRequest:
            input = ["hello"]
            
        request = MockRequest()
        
        lock_was_held = False
        def mock_get_embedding_dim():
            nonlocal lock_was_held
            lock_was_held = servicer.embed_lock.locked()
            return 128
            
        self.engine.get_embedding_dim = MagicMock(side_effect=mock_get_embedding_dim)
        self.engine.get_embeddings_batch = MagicMock(return_value=[[0.1] * 128])
        
        await servicer.Embed(request, None)
        self.assertTrue(lock_was_held, "embed_lock should be held during get_embedding_dim call in Embed")

    @patch("liva_native_engine.lib")
    @patch("liva_native_engine.should_use_mmap")
    def test_large_model_bypasses_embed_ctx_init(self, mock_should_use_mmap, mock_lib):
        mock_should_use_mmap.return_value = True
        mock_lib.llama_model_load_from_file.return_value = MagicMock()
        mock_lib.llama_model_get_vocab.return_value = MagicMock()
        mock_lib.llama_vocab_eos.return_value = 2
        mock_lib.llama_vocab_bos.return_value = 1
        mock_lib.llama_context_default_params.return_value = MagicMock()
        mock_lib.llama_init_from_model.return_value = MagicMock()
        mock_lib.llama_n_ctx.return_value = 8192
        mock_lib.llama_sampler_chain_default_params.return_value = MagicMock()
        mock_lib.llama_sampler_chain_init.return_value = MagicMock()
        
        # Test large model keyword in filename (e.g. "qwen-32b-chat.gguf")
        large_engine = LivaNativeEngine(
            model_path="qwen-32b-chat.gguf",
            n_threads=1,
            n_threads_batch=1
        )
        self.assertIsNone(large_engine.embed_ctx, "embed_ctx should be None for 32b large model")
        self.assertIsNone(large_engine.embed_ctx_params, "embed_ctx_params should be None for 32b large model")
        
        # Test non-large model (e.g. "gemma-2b.gguf")
        with patch("sys.platform", "linux"):
            normal_engine = LivaNativeEngine(
                model_path="gemma-2b.gguf",
                n_threads=1,
                n_threads_batch=1
            )
        self.assertIsNotNone(normal_engine.embed_ctx_params, "embed_ctx_params should be created for normal model")

    @patch("liva_native_engine.EngineFactory.create_engine")
    @patch("gc.collect")
    def test_engine_wrapper_hot_swap_forces_gc(self, mock_gc_collect, mock_create):
        mock_engine_old = MagicMock()
        mock_engine_new = MagicMock()
        mock_create.side_effect = [mock_engine_old, mock_engine_new]
        
        from liva_native_engine import LivaEngineWrapper
        wrapper = LivaEngineWrapper("llama.cpp", "old.gguf")
        
        mock_gc_collect.reset_mock()
        success, loaded_model, duration = wrapper.hot_swap_model("new.gguf", backend="llama.cpp")
        self.assertTrue(success)
        mock_engine_old.shutdown.assert_called_once()
        # Verify that gc.collect() was called twice
        self.assertEqual(mock_gc_collect.call_count, 2, "gc.collect() should be called exactly twice")

    @patch("liva_native_engine.lib")
    @patch("liva_native_engine.is_macos_memory_pressure")
    @patch("liva_native_engine._logger")
    def test_speculative_decoding_periodic_check(self, mock_logger, mock_is_memory_pressure, mock_lib):
        # Enable speculative decoding
        self.engine.draft_ctx = MagicMock()
        self.engine.draft_sampler = MagicMock()
        self.engine.draft_len = 5
        self.engine.n_ctx = 100
        self.engine._cached_tokens = [1, 2, 3]
        
        # We mock llama_sampler_sample to return a series of token IDs:
        # For the draft sampler: we'll return 10, 11, 12, 13, 14, 15...
        # For the target sampler: we'll return the same to simulate 100% acceptance
        draft_tokens = [10, 11, 12, 13, 14] * 5
        target_tokens = [10, 11, 12, 13, 14, 15] * 5
        
        draft_iter = iter(draft_tokens)
        target_iter = iter(target_tokens)
        
        def mock_sample(sampler, ctx, idx):
            if sampler == self.engine.draft_sampler:
                return next(draft_iter)
            else:
                return next(target_iter)
                
        mock_lib.llama_sampler_sample.side_effect = mock_sample
        mock_lib.llama_decode.return_value = 0
        self.engine.detokenize = MagicMock(return_value="t ")
        
        # First test: memory pressure is False. We generate enough tokens to trigger multiple checks.
        mock_is_memory_pressure.return_value = False
        
        self.engine._adjust_threads_hardware_adaptive = MagicMock()
        
        gen = self.engine.generate_stream([1, 2, 3], max_tokens=15)
        res = list(gen)
        
        # Check that adjust threads was called twice (at 12 and 18 tokens)
        self.assertEqual(self.engine._adjust_threads_hardware_adaptive.call_count, 2)
        
        # Second test: memory pressure is True.
        mock_is_memory_pressure.side_effect = [True, True]
        
        # Restore references so we can test reclamation
        self.engine.draft_ctx = MagicMock()
        self.engine.draft_sampler = MagicMock()
        
        draft_iter = iter(draft_tokens)
        target_iter = iter(target_tokens)
        mock_lib.llama_sampler_sample.side_effect = mock_sample
        
        mock_draft_model = MagicMock()
        self.engine.draft_model = mock_draft_model
        mock_embed_ctx = MagicMock()
        self.engine.embed_ctx = mock_embed_ctx
        self.engine.embed_memory = MagicMock()
        
        gen2 = self.engine.generate_stream([1, 2, 3], max_tokens=15)
        res2 = list(gen2)
        
        # Loop should have broken at 12 tokens, so res2 should only contain 12 tokens
        self.assertEqual(len(res2), 12)
        
        # Draft model/ctx/sampler and embed_ctx should be reclaimed (None)
        self.assertIsNone(self.engine.draft_sampler)
        self.assertIsNone(self.engine.draft_ctx)
        self.assertIsNone(self.engine.draft_model)
        self.assertIsNone(self.engine.embed_ctx)

if __name__ == "__main__":
    unittest.main()
