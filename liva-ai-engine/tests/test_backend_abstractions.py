import unittest
import sys
import asyncio
from unittest.mock import MagicMock, patch
from liva_native_engine import BaseEngine, LivaMlxEngine, EngineFactory, LivaEngineWrapper, LivaNativeEngine

class TestBackendAbstractions(unittest.IsolatedAsyncioTestCase):

    def test_base_engine_abstract(self):
        # BaseEngine is abstract, trying to instantiate it should raise TypeError
        with self.assertRaises(TypeError):
            BaseEngine()

    @patch("liva_native_engine.LivaNativeEngine")
    def test_engine_factory(self, mock_native):
        mock_instance = MagicMock(spec=LivaNativeEngine)
        mock_native.return_value = mock_instance

        # Test Native Engine creation
        engine = EngineFactory.create_engine("llama.cpp", "dummy.gguf", n_ctx=2048)
        self.assertEqual(engine, mock_instance)
        mock_native.assert_called_once_with(model_path="dummy.gguf", n_ctx=2048)

        # Test invalid engine creation
        with self.assertRaises(ValueError):
            EngineFactory.create_engine("unknown_backend", "dummy.gguf")

    @patch("liva_native_engine.EngineFactory.create_engine")
    def test_engine_wrapper_delegation(self, mock_create):
        mock_engine = MagicMock(spec=BaseEngine)
        mock_create.return_value = mock_engine

        wrapper = LivaEngineWrapper("llama.cpp", "dummy.gguf", n_ctx=2048)

        # Test tokenization delegation
        mock_engine.tokenize.return_value = [1, 2, 3]
        res = wrapper.tokenize("hello", add_special=True)
        self.assertEqual(res, [1, 2, 3])
        mock_engine.tokenize.assert_called_once_with("hello", True)

        # Test detokenization delegation
        mock_engine.detokenize.return_value = "hello"
        res = wrapper.detokenize(1)
        self.assertEqual(res, "hello")
        mock_engine.detokenize.assert_called_once_with(1)

        # Test generate_stream delegation
        mock_engine.generate_stream.return_value = (x for x in ["a", "b"])
        res = list(wrapper.generate_stream([1, 2], max_tokens=10))
        self.assertEqual(res, ["a", "b"])
        mock_engine.generate_stream.assert_called_once_with([1, 2], 10)

        # Test generate delegation
        mock_engine.generate.return_value = "ab"
        res = wrapper.generate([1, 2], max_tokens=10)
        self.assertEqual(res, "ab")
        mock_engine.generate.assert_called_once_with([1, 2], 10)

        # Test get_embedding_dim delegation
        mock_engine.get_embedding_dim.return_value = 128
        res = wrapper.get_embedding_dim()
        self.assertEqual(res, 128)
        mock_engine.get_embedding_dim.assert_called_once()

        # Test get_embeddings_batch delegation
        mock_engine.get_embeddings_batch.return_value = [[0.1, 0.2]]
        res = wrapper.get_embeddings_batch(["hello"])
        self.assertEqual(res, [[0.1, 0.2]])
        mock_engine.get_embeddings_batch.assert_called_once_with(["hello"])

    @patch("liva_native_engine.EngineFactory.create_engine")
    def test_engine_wrapper_hot_swap(self, mock_create):
        mock_engine_old = MagicMock(spec=BaseEngine)
        mock_engine_new = MagicMock(spec=BaseEngine)
        mock_create.side_effect = [mock_engine_old, mock_engine_new]

        wrapper = LivaEngineWrapper("llama.cpp", "old.gguf")
        self.assertEqual(wrapper.current_engine, mock_engine_old)

        # Perform hot swap to MLX
        success, loaded_model, duration = wrapper.hot_swap_model("new_folder", backend="mlx")
        self.assertTrue(success)
        self.assertEqual(loaded_model, "new_folder")
        mock_engine_old.shutdown.assert_called_once()
        self.assertEqual(wrapper.backend, "mlx")
        self.assertEqual(wrapper.current_engine, mock_engine_new)

    @patch("liva_native_engine.EngineFactory.create_engine")
    def test_engine_wrapper_auto_detect_backend(self, mock_create):
        mock_engine_old = MagicMock(spec=BaseEngine)
        mock_engine_new = MagicMock(spec=BaseEngine)
        mock_create.side_effect = [mock_engine_old, mock_engine_new]

        wrapper = LivaEngineWrapper("mlx", "old_folder")

        # Swap to a GGUF file with no explicit backend -> should auto-detect "llama.cpp"
        success, loaded_model, duration = wrapper.hot_swap_model("new_model.gguf")
        self.assertTrue(success)
        self.assertEqual(wrapper.backend, "llama.cpp")

        # Swap to a folder with no GGUF extension -> should auto-detect "mlx"
        mock_engine_newer = MagicMock(spec=BaseEngine)
        mock_create.side_effect = [mock_engine_newer]
        success, loaded_model, duration = wrapper.hot_swap_model("mlx_model_dir")
        self.assertTrue(success)
        self.assertEqual(wrapper.backend, "mlx")

if __name__ == "__main__":
    unittest.main()
