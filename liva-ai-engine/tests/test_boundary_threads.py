import unittest
import sys
from liva_native_engine import get_cpu_thread_counts

class TestBoundaryThreads(unittest.TestCase):
    def test_n_threads_one(self):
        """Verify get_cpu_thread_counts(1, 0) behavior on Darwin/non-Darwin."""
        res_threads, res_threads_batch = get_cpu_thread_counts(1, 0)
        if sys.platform == "darwin":
            p_cores = get_cpu_thread_counts(0)[0]
            self.assertEqual(res_threads, min(1, p_cores))
            self.assertEqual(res_threads_batch, p_cores)
        else:
            self.assertEqual(res_threads, 1)
            self.assertEqual(res_threads_batch, 1)

    def test_negative_threads(self):
        """Verify negative thread inputs default to auto-detected cores."""
        res_threads, res_threads_batch = get_cpu_thread_counts(-1, -1)
        if sys.platform == "darwin":
            p_cores = get_cpu_thread_counts(0)[0]
            self.assertEqual(res_threads, p_cores)
            self.assertEqual(res_threads_batch, p_cores)
        else:
            self.assertTrue(res_threads > 0)
            self.assertTrue(res_threads_batch > 0)

    def test_negative_batch_only(self):
        """Verify negative batch threads with positive threads."""
        res_threads, res_threads_batch = get_cpu_thread_counts(2, -5)
        if sys.platform == "darwin":
            p_cores = get_cpu_thread_counts(0)[0]
            self.assertEqual(res_threads, min(2, p_cores))
            self.assertEqual(res_threads_batch, p_cores)
        else:
            self.assertEqual(res_threads, 2)
            self.assertTrue(res_threads_batch > 0)

if __name__ == "__main__":
    unittest.main()
