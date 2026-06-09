import unittest
import sys
import asyncio
import subprocess
import os
import ctypes
from unittest.mock import patch, MagicMock

# Import the engine and thread count helper
from liva_native_engine import LivaNativeEngine, get_cpu_thread_counts

class TestMacOSExploratory(unittest.TestCase):

    # --- 1. macOS Process Monitoring "vram_guard_loop" ---
    @patch("liva_native_engine.lib")
    @patch("liva_native_engine.should_use_mmap")
    def setUp(self, mock_should_use_mmap, mock_lib):
        # Setup clean mocks for LivaNativeEngine to avoid loading libllama dynamically
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
            n_threads=1,
            n_threads_batch=1
        )
        self.engine.shutdown = MagicMock()

    @patch("sys.platform", "darwin")
    @patch("asyncio.sleep")
    @patch("subprocess.check_output")
    def test_vram_guard_path_contains_code_no_false_positive(self, mock_check_output, mock_sleep):
        """
        Verify that having LIVA itself or other safe python script running from a path containing
        the string 'code' or 'studio' does NOT trigger a false positive heavy app detection.
        """
        # Simulated ps output including LIVA running from a path with "code" and "studio" in the dir path.
        mock_check_output.return_value = (
            "COMM\n"
            "/sbin/launchd\n"
            "/Users/duongnad/code/LIVA/liva-ai-engine/venv/bin/python\n"
            "/Users/duongnad/studio/LIVA/liva-ai-engine/venv/bin/python\n"
        )
        # End loop after 1 iteration
        mock_sleep.side_effect = [None, asyncio.CancelledError()]

        try:
            asyncio.run(self.engine.vram_guard_loop())
        except asyncio.CancelledError:
            pass

        # Since it's python running under /code/ or /studio/, shutdown should NOT be triggered
        self.engine.shutdown.assert_not_called()

    @patch("sys.platform", "darwin")
    @patch("asyncio.sleep")
    @patch("subprocess.check_output")
    def test_vram_guard_actual_heavy_code_and_studio_apps(self, mock_check_output, mock_sleep):
        """
        Verify that actual VS Code (/visual studio code.app/) or Android Studio (/android studio.app/)
        DO trigger heavy app detection and shut down the engine.
        """
        # Test VS Code matching path
        mock_check_output.return_value = (
            "COMM\n"
            "/Applications/Visual Studio Code.app/Contents/MacOS/Electron\n"
        )
        mock_sleep.side_effect = [None, asyncio.CancelledError()]
        try:
            asyncio.run(self.engine.vram_guard_loop())
        except asyncio.CancelledError:
            pass
        self.engine.shutdown.assert_called_once()
        self.engine.shutdown.reset_mock()

        # Test Android Studio matching path
        mock_check_output.return_value = (
            "COMM\n"
            "/Applications/Android Studio.app/Contents/MacOS/studio\n"
        )
        mock_sleep.side_effect = [None, asyncio.CancelledError()]
        try:
            asyncio.run(self.engine.vram_guard_loop())
        except asyncio.CancelledError:
            pass
        self.engine.shutdown.assert_called_once()

    # --- 2. Decoupled Thread Count Calculations under Darwin ---
    @patch("sys.platform", "darwin")
    @patch("subprocess.run")
    def test_get_cpu_thread_counts_darwin_decoupling(self, mock_run):
        """
        Verify thread count retrieval logic correctly decouples threads and batch threads under Darwin
        with simulated P-cores.
        """
        # Mock sysctl to return 4 P-cores (hw.perflevel0.physicalcpu) and 8 total physical cores (hw.physicalcpu)
        mock_res_p = MagicMock()
        mock_res_p.stdout = "4\n"
        mock_res_p.returncode = 0

        mock_res_total = MagicMock()
        mock_res_total.stdout = "8\n"
        mock_res_total.returncode = 0

        mock_run.side_effect = [mock_res_p, mock_res_total]

        # 1. Calling (0, 0) should default both to P-cores (4, 4) on macOS
        mock_run.side_effect = [mock_res_p]
        self.assertEqual(get_cpu_thread_counts(0, 0), (4, 4))

        # 2. Calling (2, 0) should return (2, 4) since n_threads=2, and n_threads_batch=0 defaults to p_cores=4
        mock_run.side_effect = [mock_res_p]
        self.assertEqual(get_cpu_thread_counts(2, 0), (2, 4))

        # 3. Calling (6, 0) should return (4, 4) since both are capped to p_cores=4
        mock_run.side_effect = [mock_res_p]
        self.assertEqual(get_cpu_thread_counts(6, 0), (4, 4))

        # 4. Calling (0, 3) should return (4, 3) since n_threads defaults to p_cores (4), and batch is explicitly 3
        mock_run.side_effect = [mock_res_p]
        self.assertEqual(get_cpu_thread_counts(0, 3), (4, 3))

    # --- 3. Ctypes Initialization Fallbacks ---
    def test_ctypes_fallbacks_graceful_missing(self):
        """
        Verify that missing functions on the C library trigger the fallback try-except logic
        without raising any AttributeError or crash.
        """
        # Create a mock library that misses all methods
        class MockLib:
            pass

        mock_lib = MockLib()
        
        # Re-run a simplified fallback mapping logic on this mock library to test resilience
        # 1. llama_get_embeddings
        has_get_embeddings = True
        try:
            mock_lib.llama_get_embeddings.argtypes = [ctypes.c_void_p]
            mock_lib.llama_get_embeddings.restype = ctypes.POINTER(ctypes.c_float)
        except AttributeError:
            has_get_embeddings = False
        self.assertFalse(has_get_embeddings)

        # 2. llama_kv_cache_clear (recreates the fallback search)
        try:
            mock_lib.llama_kv_cache_clear.argtypes = [ctypes.c_void_p]
            mock_lib.llama_kv_cache_clear.restype = None
        except AttributeError:
            try:
                mock_lib.llama_kv_cache_clear = mock_lib.llama_memory_clear
                mock_lib.llama_kv_cache_clear.argtypes = [ctypes.c_void_p]
                mock_lib.llama_kv_cache_clear.restype = None
            except AttributeError:
                mock_lib.llama_kv_cache_clear = lambda ctx: None

        self.assertIsNotNone(mock_lib.llama_kv_cache_clear)
        # Verify call does not crash
        mock_lib.llama_kv_cache_clear(None)

        # 3. llama_kv_cache_seq_rm (recreates the fallback search)
        try:
            mock_lib.llama_kv_cache_seq_rm.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int]
            mock_lib.llama_kv_cache_seq_rm.restype = ctypes.c_bool
        except AttributeError:
            try:
                mock_lib.llama_kv_cache_seq_rm = mock_lib.llama_memory_seq_rm
                mock_lib.llama_kv_cache_seq_rm.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int]
                mock_lib.llama_kv_cache_seq_rm.restype = ctypes.c_bool
            except AttributeError:
                mock_lib.llama_kv_cache_seq_rm = lambda ctx, seq_id, p0, p1: True

        self.assertIsNotNone(mock_lib.llama_kv_cache_seq_rm)
        self.assertTrue(mock_lib.llama_kv_cache_seq_rm(None, 0, 0, 0))

    # --- 4. llama_decode Pruning RuntimeError check ---
    @patch("liva_native_engine.lib")
    def test_llama_decode_kv_pruning_fails_raises_runtime_error(self, mock_lib):
        """
        Verify that if llama_decode fails (returns non-zero) during KV cache pruning,
        a RuntimeError is correctly raised.
        """
        # Mock a model with n_ctx = 100
        self.engine.n_ctx = 100
        self.engine.model = MagicMock()
        self.engine.ctx = MagicMock()
        self.engine.sampler = MagicMock()
        self.engine.has_sampler_accept = False
        self.engine.has_sampler_reset = True
        self.engine._cached_tokens = [1] * 90

        # Mock tokenizing & sampling functions
        self.engine.tokenize = MagicMock(return_value=[1])
        mock_lib.llama_sampler_sample.return_value = 2
        mock_lib.llama_batch_free = MagicMock()
        mock_lib.llama_token_to_piece.return_value = 0

        # Dynamic trigger: when llama_batch_init is called during prompt ingestion,
        # we decrease n_ctx to 80. This bypasses the pre-generation truncation check
        # but triggers KV cache pruning (since n_past=90 >= n_ctx=80) at the start
        # of the autoregressive generation loop.
        def mock_batch_init(n_tokens, embd, n_seq_max):
            self.engine.n_ctx = 80
            return MagicMock()

        mock_lib.llama_batch_init.side_effect = mock_batch_init

        # Mock other called C methods
        mock_lib.llama_kv_cache_seq_rm = MagicMock()
        mock_lib.llama_kv_cache_seq_add = MagicMock()
        mock_lib.llama_kv_cache_defrag = MagicMock()
        mock_lib.llama_sampler_reset = MagicMock()

        # Force llama_decode to succeed during prompt ingestion (1st call, returns 0)
        # but fail during KV cache pruning (2nd call, returns 1).
        mock_lib.llama_decode.side_effect = [0, 1]

        with self.assertRaises(RuntimeError) as context:
            self.engine.generate(prompt_tokens=[1]*90, max_tokens=10)

        self.assertIn("llama_decode failed during KV cache pruning", str(context.exception))

if __name__ == "__main__":
    unittest.main()
