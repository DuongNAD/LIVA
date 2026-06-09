import unittest
import sys
import asyncio
from unittest.mock import MagicMock, patch
from liva_native_engine import LivaNativeEngine

class TestVramGuardProcess(unittest.TestCase):
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

        self.engine = LivaNativeEngine(
            model_path="dummy_path.gguf",
            n_threads=1,
            n_threads_batch=1
        )
        self.engine.shutdown = MagicMock()

    @patch("sys.platform", "darwin")
    @patch("asyncio.sleep")
    @patch("subprocess.check_output")
    def test_vram_guard_darwin_no_heavy_app(self, mock_check_output, mock_sleep):
        # Mock ps output containing normal processes
        mock_check_output.return_value = "COMM\n/sbin/launchd\n/usr/libexec/logd\n"
        
        # Stop loop after first iteration
        mock_sleep.side_effect = [None, asyncio.CancelledError()]

        try:
            asyncio.run(self.engine.vram_guard_loop())
        except asyncio.CancelledError:
            pass

        self.engine.shutdown.assert_not_called()

    @patch("sys.platform", "darwin")
    @patch("asyncio.sleep")
    @patch("subprocess.check_output")
    def test_vram_guard_darwin_heavy_app_keywords(self, mock_check_output, mock_sleep):
        # Test case-insensitivity, spaces, and path matching for keywords
        # DaVinci Resolve contains "resolve"
        mock_check_output.return_value = (
            "COMM\n"
            "/Applications/DaVinci Resolve/DaVinci Resolve.app/Contents/MacOS/DaVinci Resolve\n"
        )
        mock_sleep.side_effect = [None, asyncio.CancelledError()]

        try:
            asyncio.run(self.engine.vram_guard_loop())
        except asyncio.CancelledError:
            pass

        self.engine.shutdown.assert_called_once()

    @patch("sys.platform", "darwin")
    @patch("asyncio.sleep")
    @patch("subprocess.check_output")
    def test_vram_guard_darwin_heavy_app_exact_match(self, mock_check_output, mock_sleep):
        # Test exact match in HEAVY_APPS list (e.g. cyberpunk2077)
        mock_check_output.return_value = "COMM\n/usr/local/bin/cyberpunk2077.app\n"
        mock_sleep.side_effect = [None, asyncio.CancelledError()]

        try:
            asyncio.run(self.engine.vram_guard_loop())
        except asyncio.CancelledError:
            pass

        self.engine.shutdown.assert_called_once()

    @patch("sys.platform", "darwin")
    @patch("asyncio.sleep")
    @patch("subprocess.check_output")
    def test_vram_guard_darwin_invalid_subprocess_output(self, mock_check_output, mock_sleep):
        # Test exception raised by subprocess.check_output is caught gracefully
        mock_check_output.side_effect = Exception("Subprocess failed")
        mock_sleep.side_effect = [None, asyncio.CancelledError()]

        try:
            asyncio.run(self.engine.vram_guard_loop())
        except asyncio.CancelledError:
            pass

        # Should not crash the loop or trigger shutdown
        self.engine.shutdown.assert_not_called()

    @patch("sys.platform", "darwin")
    @patch("asyncio.sleep")
    @patch("subprocess.check_output")
    def test_vram_guard_darwin_empty_output(self, mock_check_output, mock_sleep):
        # Test empty ps output
        mock_check_output.return_value = ""
        mock_sleep.side_effect = [None, asyncio.CancelledError()]

        try:
            asyncio.run(self.engine.vram_guard_loop())
        except asyncio.CancelledError:
            pass

        self.engine.shutdown.assert_not_called()

if __name__ == "__main__":
    unittest.main()
