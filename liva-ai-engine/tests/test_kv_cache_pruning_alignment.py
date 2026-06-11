import unittest
from unittest.mock import MagicMock, patch
import os
import sys
from liva_native_engine import LivaNativeEngine

class TestKvCachePruningAlignment(unittest.TestCase):
    def setUp(self):
        # Setup the global mock patchers
        self.lib_patcher = patch("liva_native_engine.lib")
        self.mock_lib = self.lib_patcher.start()
        self.addCleanup(self.lib_patcher.stop)
        
        self.mmap_patcher = patch("liva_native_engine.should_use_mmap")
        self.mock_should_use_mmap = self.mmap_patcher.start()
        self.addCleanup(self.mmap_patcher.stop)
        
        # Setup common mock behavior
        self.mock_should_use_mmap.return_value = True
        self.mock_lib.llama_model_load_from_file.return_value = MagicMock()
        self.mock_lib.llama_model_get_vocab.return_value = MagicMock()
        self.mock_lib.llama_vocab_eos.return_value = 2
        self.mock_lib.llama_vocab_bos.return_value = 1
        self.mock_lib.llama_context_default_params.return_value = MagicMock()
        self.mock_lib.llama_init_from_model.return_value = MagicMock()
        self.mock_lib.llama_n_ctx.return_value = 100
        self.mock_lib.llama_sampler_chain_default_params.return_value = MagicMock()
        self.mock_lib.llama_sampler_chain_init.return_value = MagicMock()
        self.mock_lib.llama_token_to_piece.return_value = 0
        
        self.engine = LivaNativeEngine(
            model_path="dummy_path.gguf",
            n_ctx=100,
            n_threads=1,
            n_threads_batch=1
        )
        self.engine.shutdown = MagicMock()

    def test_speculative_pruning_boundary_trigger(self):
        """
        Verify that sliding window pruning is triggered when n_past + (H + 1) > n_ctx
        under speculative decoding, even if n_past < n_ctx.
        """
        # Enable speculative decoding mock settings
        self.engine.draft_ctx = MagicMock()
        self.engine.draft_sampler = MagicMock()
        self.engine.sampler = MagicMock()
        self.engine.has_sampler_accept = True
        self.engine.has_sampler_reset = True
        
        # S = min(512, 100 // 8) = 12
        # K = min(512, 100 // 8) = 12
        # n_past is close to boundary.
        # H = 5, max_add = 6. n_past + 6 > 100 => n_past >= 95 triggers pruning.
        # We start with 95 cached tokens.
        self.engine._cached_tokens = [1] * 95
        self.engine.tokenize = MagicMock(return_value=[1])
        
        self.mock_lib.llama_sampler_sample.return_value = 2
        self.mock_lib.llama_batch_free = MagicMock()
        self.mock_lib.llama_decode.return_value = 0
        
        self.mock_lib.llama_kv_cache_seq_rm = MagicMock()
        self.mock_lib.llama_kv_cache_seq_add = MagicMock()
        self.mock_lib.llama_kv_cache_defrag = MagicMock()
        
        gen = self.engine.generate_stream(prompt_tokens=[1]*95, max_tokens=1)
        next(gen, None)
        
        # Verify that llama_kv_cache_seq_rm was called to prune
        self.mock_lib.llama_kv_cache_seq_rm.assert_any_call(self.engine.ctx, 0, 12, 24)
        self.mock_lib.llama_kv_cache_seq_rm.assert_any_call(self.engine.draft_ctx, 0, 12, 24)
        # Verify that the new last token slot was cleared to prevent duplicate position entries in KV cache
        self.mock_lib.llama_kv_cache_seq_rm.assert_any_call(self.engine.ctx, 0, 82, 83)
        self.mock_lib.llama_kv_cache_seq_rm.assert_any_call(self.engine.draft_ctx, 0, 82, 83)

    def test_standard_decode_failure_raises_runtime_error(self):
        """
        Verify that llama_decode failure in the standard non-speculative path
        raises RuntimeError and clears _cached_tokens.
        """
        self.engine._cached_tokens = [1, 2, 3]
        self.engine.tokenize = MagicMock(return_value=[1])
        self.mock_lib.llama_sampler_sample.return_value = 10
        
        # Initial prompt decode succeeds (returns 0), next autoregressive decode fails (returns 1)
        self.mock_lib.llama_decode.side_effect = [0, 1]
        
        gen = self.engine.generate_stream(prompt_tokens=[1, 2, 3], max_tokens=5)
        
        with self.assertRaises(RuntimeError) as context:
            list(gen)
            
        self.assertIn("llama_decode failed (rc=1)", str(context.exception))
        self.assertIsNone(self.engine._cached_tokens)

    def test_speculative_batch_verification_failure_raises_runtime_error(self):
        """
        Verify that target model llama_decode failure during speculative batch verification
        raises RuntimeError and clears _cached_tokens.
        """
        self.engine.draft_ctx = MagicMock()
        self.engine.draft_sampler = MagicMock()
        self.engine.has_sampler_accept = True
        self.engine.has_sampler_reset = True
        
        self.engine._cached_tokens = [1, 2, 3]
        self.engine.tokenize = MagicMock(return_value=[1])
        self.mock_lib.llama_sampler_sample.return_value = 10
        
        # Prefill: 1 decode for ctx, 1 for draft_ctx. Total 2.
        # Draft generation loop: 5 decodes for draft_ctx.
        # Target batch decode: 1 decode for ctx (fails!).
        self.mock_lib.llama_decode.side_effect = [0, 0] + [0]*5 + [1]
        
        gen = self.engine.generate_stream(prompt_tokens=[1, 2, 3], max_tokens=5)
        
        with self.assertRaises(RuntimeError) as context:
            list(gen)
            
        self.assertIn("Target model llama_decode failed during speculative batch verification (rc=1)", str(context.exception))
        self.assertIsNone(self.engine._cached_tokens)

    def test_speculative_fallback_target_failure_raises_runtime_error(self):
        """
        Verify that target model llama_decode failure during speculative fallback
        raises RuntimeError and clears _cached_tokens.
        """
        self.engine.draft_ctx = MagicMock()
        self.engine.draft_sampler = MagicMock()
        self.engine.has_sampler_accept = True
        self.engine.has_sampler_reset = True
        
        self.engine._cached_tokens = [1, 2, 3]
        self.engine.tokenize = MagicMock(return_value=[1])
        
        # Set draft length to 0 to trigger fallback path immediately
        self.engine.draft_len = 0
        
        # Target samples 10
        self.mock_lib.llama_sampler_sample.return_value = 10
        
        # Prefill: 0, 0.
        # Target fallback decode fails (1).
        self.mock_lib.llama_decode.side_effect = [0, 0, 1]
        
        gen = self.engine.generate_stream(prompt_tokens=[1, 2, 3], max_tokens=5)
        
        with self.assertRaises(RuntimeError) as context:
            list(gen)
            
        self.assertIn("Target model llama_decode failed during speculative fallback (rc=1)", str(context.exception))
        self.assertIsNone(self.engine._cached_tokens)

    def test_speculative_fallback_draft_failure_raises_runtime_error(self):
        """
        Verify that draft model llama_decode failure during speculative fallback
        raises RuntimeError and clears _cached_tokens.
        """
        self.engine.draft_ctx = MagicMock()
        self.engine.draft_sampler = MagicMock()
        self.engine.has_sampler_accept = True
        self.engine.has_sampler_reset = True
        
        self.engine._cached_tokens = [1, 2, 3]
        self.engine.tokenize = MagicMock(return_value=[1])
        
        # Set draft length to 0 to trigger fallback path immediately
        self.engine.draft_len = 0
        
        # Target samples 10
        self.mock_lib.llama_sampler_sample.return_value = 10
        
        # Prefill: 0, 0.
        # Fallback target decode succeeds (0).
        # Fallback draft decode fails (1).
        self.mock_lib.llama_decode.side_effect = [0, 0, 0, 1]
        
        gen = self.engine.generate_stream(prompt_tokens=[1, 2, 3], max_tokens=5)
        
        with self.assertRaises(RuntimeError) as context:
            list(gen)
            
        self.assertIn("Draft model llama_decode failed during speculative fallback (rc=1)", str(context.exception))
        self.assertIsNone(self.engine._cached_tokens)

    def test_speculative_alignment_target_failure_raises_runtime_error(self):
        """
        Verify that target model llama_decode failure during speculative alignment
        raises RuntimeError and clears _cached_tokens.
        """
        self.engine.draft_ctx = MagicMock()
        self.engine.draft_sampler = MagicMock()
        self.engine.has_sampler_accept = True
        self.engine.has_sampler_reset = True
        
        self.engine._cached_tokens = [1, 2, 3]
        self.engine.tokenize = MagicMock(return_value=[1])
        
        # Draft sampler returns draft token 10 then 2 (EOS). Target sampler returns 10 (match) first, then 11 (corrected).
        self.mock_lib.llama_sampler_sample.side_effect = [10, 2, 10, 11]
        
        # Prefill: 0, 0.
        # Draft loop decode: 0.
        # Target verification batch decode: 0.
        # Alignment target decode: 1.
        self.mock_lib.llama_decode.side_effect = [0, 0, 0, 0, 1]
        
        gen = self.engine.generate_stream(prompt_tokens=[1, 2, 3], max_tokens=5)
        
        with self.assertRaises(RuntimeError) as context:
            list(gen)
            
        self.assertIn("Target model llama_decode failed during speculative alignment (rc=1)", str(context.exception))
        self.assertIsNone(self.engine._cached_tokens)

    def test_speculative_alignment_draft_failure_raises_runtime_error(self):
        """
        Verify that draft model llama_decode failure during speculative alignment
        raises RuntimeError and clears _cached_tokens.
        """
        self.engine.draft_ctx = MagicMock()
        self.engine.draft_sampler = MagicMock()
        self.engine.has_sampler_accept = True
        self.engine.has_sampler_reset = True
        
        self.engine._cached_tokens = [1, 2, 3]
        self.engine.tokenize = MagicMock(return_value=[1])
        
        # Draft sampler returns draft token 10 then 2 (EOS). Target sampler returns 10 (match) first, then 11 (corrected).
        self.mock_lib.llama_sampler_sample.side_effect = [10, 2, 10, 11]
        
        # Prefill: 0, 0.
        # Draft loop decode: 0.
        # Target verification batch decode: 0.
        # Alignment target decode: 0.
        # Alignment draft decode: 1.
        self.mock_lib.llama_decode.side_effect = [0, 0, 0, 0, 0, 1]
        
        gen = self.engine.generate_stream(prompt_tokens=[1, 2, 3], max_tokens=5)
        
        with self.assertRaises(RuntimeError) as context:
            list(gen)
            
        self.assertIn("Draft model llama_decode failed during speculative alignment (rc=1)", str(context.exception))
        self.assertIsNone(self.engine._cached_tokens)

    def test_draft_model_use_mmap_respects_should_use_mmap(self):
        """
        Verify that draft model use_mmap matches the value of should_use_mmap().
        """
        with patch.dict("os.environ", {"LIVA_ENABLE_SPECULATIVE": "true", "LIVA_DRAFT_MODEL_NAME": "draft.gguf"}):
            with patch("os.path.exists", return_value=True):
                mock_params = MagicMock()
                self.mock_lib.llama_model_default_params.return_value = mock_params
                self.mock_lib.llama_model_load_from_file.return_value = MagicMock()
                
                # Test with should_use_mmap returning False
                self.mock_should_use_mmap.return_value = False
                self.engine._init_draft_model(100, -1, 512, 4, True)
                self.assertFalse(mock_params.use_mmap)
                
                # Test with should_use_mmap returning True
                self.mock_should_use_mmap.return_value = True
                self.engine._init_draft_model(100, -1, 512, 4, True)
                self.assertTrue(mock_params.use_mmap)

if __name__ == "__main__":
    unittest.main()
