import unittest
from unittest.mock import MagicMock, patch
from liva_native_engine import LivaNativeEngine

class TestKvCacheException(unittest.TestCase):
    @patch("liva_native_engine.lib")
    @patch("liva_native_engine.should_use_mmap")
    def test_kv_cache_cleanup_on_exception(self, mock_should_use_mmap, mock_lib):
        # Mock C library to prevent loading actual DLL
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

        engine = LivaNativeEngine(
            model_path="dummy_path.gguf",
            n_threads=1,
            n_threads_batch=1
        )
        
        # Manually set some cached tokens to simulate a previous generation
        engine._cached_tokens = [1, 2, 3]

        # Mock _generate_stream_unsafe to raise an exception
        def mock_generate_stream_unsafe(prompt_tokens, max_tokens):
            # yield something, then raise exception
            yield "token1"
            raise RuntimeError("Mock generation failure")

        engine._generate_stream_unsafe = mock_generate_stream_unsafe

        # Calling generate_stream should yield token1, then throw RuntimeError,
        # and engine._cached_tokens must be set to None.
        gen = engine.generate_stream([1, 2, 3], max_tokens=10)
        
        # First yield should succeed
        self.assertEqual(next(gen), "token1")
        
        # Next step should throw
        with self.assertRaises(RuntimeError) as context:
            next(gen)
        
        self.assertEqual(str(context.exception), "Mock generation failure")
        self.assertIsNone(engine._cached_tokens)

if __name__ == "__main__":
    unittest.main()
