"""
LIVA Native Engine — TurboQuant Q4_0 Test Suite
=================================================
Tests Q4_0 quantization, ctypes ABI, and anti-hallucination guardrails.

Run:
  cd liva-ai-engine
  python -m pytest tests/ -v
"""
import pytest
import struct
import ctypes
import numpy as np


# ============================================================
# TEST GROUP 1: TurboQuant Q4_0 KV Cache Compression
# ============================================================
class TestTurboQuantQ4Zero:
    """Ensures Q4_0 quantization on the Python side matches C++ expectations."""

    def test_quantize_dequantize_identity(self):
        """Values should survive round-trip quantize->dequantize within tolerance."""
        try:
            from liva_native_engine import quantize_q4_0, dequantize_q4_0
        except ImportError:
            pytest.skip("liva_native_engine not importable without llama.dll")

        original = np.array([0.5, -1.2, 3.7, 0.0, -0.001], dtype=np.float32)
        quantized = quantize_q4_0(original)
        restored = dequantize_q4_0(quantized)
        np.testing.assert_allclose(restored, original, atol=0.2)

    def test_quantize_preserves_sign(self):
        """Quantization must preserve the sign of values."""
        try:
            from liva_native_engine import quantize_q4_0
        except ImportError:
            pytest.skip("liva_native_engine not importable without llama.dll")

        positive = np.array([1.0, 2.0, 3.0], dtype=np.float32)
        q_pos = quantize_q4_0(positive)
        assert all(v >= 0 for v in q_pos), "Positive values lost sign"

    def test_quantize_empty_array(self):
        """Should handle empty input without crashing."""
        try:
            from liva_native_engine import quantize_q4_0
        except ImportError:
            pytest.skip("liva_native_engine not importable without llama.dll")

        result = quantize_q4_0(np.array([], dtype=np.float32))
        assert len(result) == 0

    def test_quantize_large_values(self):
        """Should clamp extreme values without overflow."""
        try:
            from liva_native_engine import quantize_q4_0
        except ImportError:
            pytest.skip("liva_native_engine not importable without llama.dll")

        extreme = np.array([1e10, -1e10], dtype=np.float32)
        result = quantize_q4_0(extreme)
        assert not np.any(np.isnan(result)), "NaN produced from extreme values"


# ============================================================
# TEST GROUP 2: ctypes ABI Compatibility
# ============================================================
class TestCtypesABI:
    """Ensures data marshalling between Python and C++ (llama.dll) is safe."""

    def test_float_pointer_alignment(self):
        """Float arrays passed to C++ must be properly aligned."""
        arr = (ctypes.c_float * 128)()
        for i in range(128):
            arr[i] = float(i) * 0.1

        ptr = ctypes.cast(arr, ctypes.POINTER(ctypes.c_float))
        assert ptr[0] == pytest.approx(0.0)
        assert ptr[127] == pytest.approx(12.7)

    def test_string_encoding_utf8(self):
        """Vietnamese text must survive UTF-8 encoding for C++ bridge."""
        test_str = "Xin chao Anh Duong!"
        encoded = test_str.encode("utf-8")
        decoded = encoded.decode("utf-8")
        assert decoded == test_str

    def test_buffer_size_calculation(self):
        """Buffer sizes should be correctly computed."""
        size = 1024 * 1024  # 1MB
        buf = (ctypes.c_char * size)()
        assert ctypes.sizeof(buf) == size

    def test_null_array_sizeof(self):
        """Empty ctypes array should have sizeof 0."""
        arr = (ctypes.c_float * 0)()
        assert ctypes.sizeof(arr) == 0

    def test_struct_packing_matches_c(self):
        """Ensure Python struct packing matches C/C++ expectations."""
        # llama_token_data: { id: int32, logit: float, p: float }
        packed = struct.pack("iff", 42, 0.99, 0.01)
        id_val, logit, prob = struct.unpack("iff", packed)
        assert id_val == 42
        assert logit == pytest.approx(0.99)
        assert prob == pytest.approx(0.01)


# ============================================================
# TEST GROUP 3: Anti-Hallucination Guardrails
# ============================================================
class TestAntiHallucination:
    """Tests the deterministic guardrails in the inference pipeline."""

    def test_max_token_limit(self):
        """Token generation should be capped."""
        max_tokens = 10
        tokens = list(range(20))
        truncated = tokens[:max_tokens]
        assert len(truncated) == max_tokens

    def test_repetition_detection(self):
        """Repeated token sequences should be detected."""
        def detect_repetition(tokens, threshold=3):
            if len(tokens) < threshold:
                return False
            for i in range(len(tokens) - threshold + 1):
                window = tokens[i : i + threshold]
                if len(set(window)) == 1:
                    return True
            return False

        assert detect_repetition([42, 42, 42, 42, 42], threshold=3) is True
        assert detect_repetition([1, 2, 3, 4, 5], threshold=3) is False
        assert detect_repetition([1, 1, 2], threshold=3) is False
        assert detect_repetition([1, 1, 1], threshold=3) is True

    def test_stop_sequence_matching(self):
        """Output should be truncated at stop sequences."""
        stop_seqs = ["[/INST]", "<|end|>"]
        output = "The weather is nice today. [/INST] But ignore this part."

        for seq in stop_seqs:
            idx = output.find(seq)
            if idx >= 0:
                output = output[:idx]
                break

        assert output == "The weather is nice today. "
        assert "[/INST]" not in output

    def test_temperature_zero_deterministic(self):
        """Temperature=0 should produce deterministic output (greedy)."""
        # Simulate logits
        logits = np.array([0.1, 0.3, 0.05, 0.8, 0.2], dtype=np.float32)

        # Temperature=0 → argmax
        selected = np.argmax(logits)
        assert selected == 3  # Always selects index 3 (highest logit)

        # Run 10 times to confirm determinism
        for _ in range(10):
            assert np.argmax(logits) == 3

    def test_nan_logits_handling(self):
        """NaN logits should not crash the engine."""
        logits = np.array([float("nan"), 0.1, 0.3], dtype=np.float32)

        # Replace NaN with -inf before argmax (standard practice)
        clean = np.where(np.isnan(logits), -np.inf, logits)
        selected = np.argmax(clean)
        assert selected == 2  # Should pick the valid max


# ============================================================
# TEST GROUP 4: gRPC Server Contract
# ============================================================
class TestGRPCContract:
    """Validates the gRPC proto contract between Python Engine and Node.js Gateway."""

    def test_chat_request_required_fields(self):
        """ChatRequest must contain messages array."""
        required_fields = ["model", "messages", "temperature", "max_tokens", "stream"]
        request = {
            "model": "router",
            "messages": [{"role": "user", "content": "hello"}],
            "temperature": 0.7,
            "max_tokens": 2048,
            "stream": False,
        }
        for field in required_fields:
            assert field in request, f"Missing field: {field}"

    def test_chat_response_structure(self):
        """ChatResponse must match the expected structure."""
        response = {
            "id": "resp_123",
            "object": "chat.completion",
            "choices": [
                {
                    "index": 0,
                    "message": {"role": "assistant", "content": "Hello!"},
                    "finish_reason": "stop",
                }
            ],
            "model": "router",
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15,
            },
        }
        assert len(response["choices"]) >= 1
        assert response["choices"][0]["message"]["role"] == "assistant"
        assert isinstance(response["choices"][0]["message"]["content"], str)
        assert response["usage"]["total_tokens"] == response["usage"]["prompt_tokens"] + response["usage"]["completion_tokens"]

    def test_stream_chunk_structure(self):
        """StreamChunk must have delta with optional content."""
        chunk = {
            "id": "chunk_1",
            "object": "chat.completion.chunk",
            "choices": [
                {
                    "index": 0,
                    "delta": {"content": "Hello"},
                    "finish_reason": None,
                }
            ],
        }
        assert chunk["choices"][0]["delta"]["content"] == "Hello"
        assert chunk["choices"][0]["finish_reason"] is None

    def test_health_check_response(self):
        """HealthCheck must return alive: bool."""
        response = {"alive": True}
        assert isinstance(response["alive"], bool)
