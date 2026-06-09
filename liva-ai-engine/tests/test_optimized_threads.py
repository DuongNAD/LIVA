import sys
import unittest
from unittest.mock import patch, MagicMock
import subprocess

from liva_native_engine import get_cpu_thread_counts

class TestOptimizedThreads(unittest.TestCase):
    def test_explicit_threads(self):
        """If n_threads > 0 and n_threads_batch > 0, return (n_threads, n_threads_batch) (capped on macOS)"""
        if sys.platform == "darwin":
            p_cores = get_cpu_thread_counts(0)[0]
            self.assertEqual(get_cpu_thread_counts(4, 4), (min(4, p_cores), min(4, p_cores)))
            self.assertEqual(get_cpu_thread_counts(8, 12), (min(8, p_cores), min(12, p_cores)))
        else:
            self.assertEqual(get_cpu_thread_counts(4, 4), (4, 4))
            self.assertEqual(get_cpu_thread_counts(8, 12), (8, 12))

    def test_decoupled_defaults(self):
        """If n_threads_batch is 0 but n_threads > 0, default n_threads_batch to n_threads (on macOS, capped to p_cores)"""
        if sys.platform == "darwin":
            p_cores = get_cpu_thread_counts(0)[0]
            self.assertEqual(get_cpu_thread_counts(4, 0), (min(4, p_cores), p_cores))
            self.assertEqual(get_cpu_thread_counts(6), (min(6, p_cores), p_cores))
        else:
            self.assertEqual(get_cpu_thread_counts(4, 0), (4, 4))
            self.assertEqual(get_cpu_thread_counts(6), (6, 6))

    @patch("sys.platform", "linux")
    @patch("os.cpu_count")
    def test_partial_decoupling(self, mock_cpu_count):
        """If n_threads is 0 but n_threads_batch > 0, default n_threads to fallback P-cores"""
        mock_cpu_count.return_value = 12
        self.assertEqual(get_cpu_thread_counts(0, 8), (6, 8))


    @patch("sys.platform", "darwin")
    @patch("subprocess.run")
    def test_macos_auto_detect_success(self, mock_run):
        """On macOS, if sysctl succeeds, return (p_cores, p_cores) when n_threads_batch <= 0"""
        mock_res_p = MagicMock()
        mock_res_p.stdout = "6\n"
        mock_res_p.returncode = 0

        mock_res_total = MagicMock()
        mock_res_total.stdout = "8\n"
        mock_res_total.returncode = 0

        mock_run.side_effect = [mock_res_p, mock_res_total]

        res = get_cpu_thread_counts(0)
        self.assertEqual(res, (6, 6))

    @patch("sys.platform", "darwin")
    @patch("subprocess.run")
    @patch("os.cpu_count")
    def test_macos_auto_detect_failure_fallback(self, mock_cpu_count, mock_run, *args):
        """On macOS, if sysctl fails, fallback to logical cores counts and cap batch to p_cores"""
        mock_run.side_effect = Exception("sysctl failed")
        mock_cpu_count.return_value = 10

        res = get_cpu_thread_counts(0)
        self.assertEqual(res, (5, 5))

    @patch("sys.platform", "linux")
    @patch("os.cpu_count")
    def test_other_platforms_fallback(self, mock_cpu_count):
        """On other platforms, return (max(1, logical_cores // 2), logical_cores)"""
        mock_cpu_count.return_value = 12
        self.assertEqual(get_cpu_thread_counts(0), (6, 12))

        mock_cpu_count.return_value = 1
        self.assertEqual(get_cpu_thread_counts(0), (1, 1))


if __name__ == "__main__":
    unittest.main()
