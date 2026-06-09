import unittest
import sys
from unittest.mock import MagicMock, patch
from liva_native_engine import LivaNativeEngine, HAS_SET_N_THREADS

class TestAdaptiveTuning(unittest.TestCase):
    @patch("liva_native_engine.lib")
    @patch("liva_native_engine.should_use_mmap")
    def setUp(self, mock_should_use_mmap, mock_lib):
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

        self.engine = LivaNativeEngine(
            model_path="dummy_path.gguf",
            n_threads=4,
            n_threads_batch=4
        )
        self.engine.shutdown = MagicMock()

    @patch("liva_native_engine.lib")
    @patch("psutil.cpu_percent")
    @patch("liva_native_engine.HAS_SET_N_THREADS", True)
    def test_adjust_threads_high_cpu(self, mock_cpu_percent, mock_lib):
        # Set CPU load to 85%
        mock_cpu_percent.return_value = 85.0
        
        # Call adjust method
        self.engine._adjust_threads_hardware_adaptive()
        
        # Verify lib.llama_set_n_threads was called with (4-2) = 2 threads
        mock_lib.llama_set_n_threads.assert_any_call(self.engine.ctx, 2, 2)

    @patch("liva_native_engine.lib")
    @patch("psutil.cpu_percent")
    @patch("liva_native_engine.HAS_SET_N_THREADS", True)
    def test_adjust_threads_low_cpu(self, mock_cpu_percent, mock_lib):
        # Set CPU load to 45%
        mock_cpu_percent.return_value = 45.0
        
        # Call adjust method
        self.engine._adjust_threads_hardware_adaptive()
        
        # Verify lib.llama_set_n_threads was called with defaults (4, 4)
        mock_lib.llama_set_n_threads.assert_any_call(self.engine.ctx, 4, 4)

    @patch("liva_native_engine.lib")
    @patch("liva_native_engine.HAS_SET_N_THREADS", True)
    def test_restore_threads_defaults(self, mock_lib):
        self.engine._restore_threads_defaults()
        mock_lib.llama_set_n_threads.assert_any_call(self.engine.ctx, 4, 4)

    @patch("liva_native_engine.lib")
    @patch("liva_native_engine.is_macos_memory_pressure")
    def test_memory_pressure_reclaim(self, mock_is_memory_pressure, mock_lib):
        # Simulate memory pressure is active
        mock_is_memory_pressure.return_value = True
        
        # Initialize mock values for contexts so we can verify they are cleared/freed
        mock_draft_sampler = MagicMock()
        mock_draft_ctx = MagicMock()
        mock_draft_model = MagicMock()
        mock_embed_ctx = MagicMock()
        
        # Mock functions called during stream generation loop
        mock_lib.llama_sampler_sample.return_value = 10
        mock_lib.llama_vocab_get_text.return_value = b"token"
        mock_lib.llama_decode.return_value = 0
        
        # Mock detokenize directly on the engine to avoid C-level string buffer conversion errors
        self.engine.detokenize = MagicMock(return_value="token")
        
        # Start the generator (use_speculative is False because draft_ctx is None initially)
        gen = self.engine.generate_stream([1, 2, 3], max_tokens=20)
        
        # Consume 1 token to initialize the generator loop and execute the first decode
        first_token = next(gen)
        
        # Now set the draft/embed context properties so the reclaim logic can see them
        self.engine.draft_sampler = mock_draft_sampler
        self.engine.draft_ctx = mock_draft_ctx
        self.engine.draft_model = mock_draft_model
        self.engine.embed_ctx = mock_embed_ctx
        self.engine.embed_memory = MagicMock()
        
        # Consume the rest of the stream (which will trigger memory pressure reclaim at token 8)
        outputs = [first_token] + list(gen)
        
        # Verify that memory pressure triggered the reclamation flow
        mock_lib.llama_kv_cache_clear.assert_called_with(self.engine.ctx)
        mock_lib.llama_sampler_free.assert_any_call(mock_draft_sampler)
        mock_lib.llama_free.assert_any_call(mock_draft_ctx)
        mock_lib.llama_model_free.assert_any_call(mock_draft_model)
        mock_lib.llama_free.assert_any_call(mock_embed_ctx)
        
        self.assertIsNone(self.engine.draft_sampler)
        self.assertIsNone(self.engine.draft_ctx)
        self.assertIsNone(self.engine.draft_model)
        self.assertIsNone(self.engine.embed_ctx)
        self.assertIsNone(self.engine.embed_memory)

    @patch("liva_native_engine.lib")
    @patch("liva_native_engine.is_macos_memory_pressure")
    def test_concurrent_memory_pressure_reclaim(self, mock_is_memory_pressure, mock_lib):
        import numpy as np
        import threading
        import time

        # Simulate memory pressure is active
        mock_is_memory_pressure.return_value = True

        # Initialize mock values for contexts so we can verify they are cleared/freed
        mock_draft_sampler = MagicMock()
        mock_draft_ctx = MagicMock()
        mock_draft_model = MagicMock()
        mock_embed_ctx = MagicMock()
        mock_ctx = MagicMock()

        self.engine.ctx = mock_ctx
        self.engine.draft_sampler = None
        self.engine.draft_ctx = None
        self.engine.draft_model = None
        self.engine.embed_ctx = None
        self.engine.embed_memory = None

        # Mock C library calls
        mock_lib.llama_sampler_sample.return_value = 10
        mock_lib.llama_vocab_get_text.return_value = b"token"
        mock_lib.llama_decode.return_value = 0
        mock_lib.llama_n_ctx.return_value = 1024
        mock_lib.llama_n_embd.return_value = 128
        mock_batch = MagicMock()
        mock_lib.llama_batch_init.return_value = mock_batch

        # Mock detokenize and tokenize
        self.engine.detokenize = MagicMock(return_value="token")
        self.engine.tokenize = MagicMock(return_value=[1, 2, 3])

        # Synchronization events
        generator_ready = threading.Event()
        embed_started = threading.Event()

        # Mock llama_get_embeddings/llama_get_embeddings_seq to block until stream generation hits memory pressure
        def mock_get_embeddings_side_effect(*args, **kwargs):
            embed_started.set()
            # Give the generator thread time to run and trigger memory pressure
            time.sleep(0.1)
            return MagicMock()
        mock_lib.llama_get_embeddings.side_effect = mock_get_embeddings_side_effect
        mock_lib.llama_get_embeddings_seq.side_effect = mock_get_embeddings_side_effect

        # Thread for generate_stream
        generator_exceptions = []
        def run_generator():
            try:
                gen = self.engine.generate_stream([1, 2, 3], max_tokens=20)
                first_token = next(gen)
                
                # Now set the draft/embed context properties so they can be reclaimed
                self.engine.draft_sampler = mock_draft_sampler
                self.engine.draft_ctx = mock_draft_ctx
                self.engine.draft_model = mock_draft_model
                self.engine.embed_ctx = mock_embed_ctx
                self.engine.embed_memory = MagicMock()
                
                generator_ready.set()
                
                # Wait a tiny bit to let the main thread start the embedding thread
                time.sleep(0.02)
                
                list(gen)
            except Exception as e:
                generator_exceptions.append(e)
                generator_ready.set()

        # Thread for get_embeddings_batch
        embed_exceptions = []
        embed_results = []
        def run_embed():
            try:
                with patch("numpy.ctypeslib.as_array") as mock_as_array:
                    mock_as_array.return_value = np.zeros(128)
                    res = self.engine.get_embeddings_batch(["hello"])
                    embed_results.append(res)
            except Exception as e:
                embed_exceptions.append(e)

        t_gen = threading.Thread(target=run_generator)
        t_embed = threading.Thread(target=run_embed)

        # Start generator thread and wait until properties are initialized
        t_gen.start()
        generator_ready.wait(timeout=2.0)

        # Start embedding thread which will see embed_ctx != None and enter the locked block
        t_embed.start()
        embed_started.wait(timeout=2.0)

        t_embed.join()
        t_gen.join()

        # Verify no exceptions occurred
        self.assertEqual(generator_exceptions, [])
        self.assertEqual(embed_exceptions, [])

        # Ensure llama_free was called to free embed_ctx
        mock_lib.llama_free.assert_any_call(mock_embed_ctx)

        # Ensure properties are cleared
        self.assertIsNone(self.engine.embed_ctx)
        self.assertIsNone(self.engine.embed_memory)

if __name__ == "__main__":
    unittest.main()
