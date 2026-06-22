"""Unit tests for Whisper STT server endpoints and device detection."""
import unittest
import sys
import os
import io
import wave
import numpy as np
from unittest.mock import MagicMock, patch, mock_open

# Ensure the root directory is in sys.path
sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from fastapi.testclient import TestClient
import whisper_stt_server
from whisper_stt_server import app

def create_mock_wav(sample_rate=16000, duration_seconds=0.2):
    """Generate mock WAV bytes for testing."""
    num_samples = int(sample_rate * duration_seconds)
    t = np.linspace(0, duration_seconds, num_samples, endpoint=False)
    data = np.sin(2 * np.pi * 440.0 * t)
    int_data = (data * 32767.0).astype(np.int16)
    
    out_io = io.BytesIO()
    with wave.open(out_io, 'wb') as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(sample_rate)
        wav.writeframes(int_data.tobytes())
        
    return out_io.getvalue()

class TestWhisperEndpoints(unittest.TestCase):
    def setUp(self):
        # Create a mock Whisper model
        self.mock_model = MagicMock()
        mock_segment = MagicMock()
        mock_segment.text = "Hello Liva"
        self.mock_model.transcribe.return_value = ([mock_segment], MagicMock())
        
        # Patch the global model and model_loaded in whisper_stt_server
        self.model_patcher = patch('whisper_stt_server.model', self.mock_model)
        self.model_loaded_patcher = patch('whisper_stt_server.model_loaded', True)
        self.model_patcher.start()
        self.model_loaded_patcher.start()
        
        self.client = TestClient(app)

    def tearDown(self):
        self.model_patcher.stop()
        self.model_loaded_patcher.stop()

    def test_root_endpoint(self):
        """Test the root endpoint returns correct status information."""
        response = self.client.get("/")
        self.assertEqual(response.status_code, 200)
        data = response.json()
        self.assertEqual(data["service"], "LIVA Whisper STT")
        self.assertEqual(data["ready"], True)

    def test_health_endpoint(self):
        """Test the health check endpoint."""
        response = self.client.get("/health")
        self.assertEqual(response.status_code, 200)
        data = response.json()
        self.assertEqual(data["status"], "ok")
        self.assertEqual(data["model"], "small")

    def test_transcribe_json_format(self):
        """Test transcription endpoint returning JSON response."""
        wav_bytes = create_mock_wav()
        response = self.client.post(
            "/v1/audio/transcriptions",
            files={"file": ("test.wav", wav_bytes, "audio/wav")},
            data={"response_format": "json"}
        )
        self.assertEqual(response.status_code, 200)
        data = response.json()
        self.assertEqual(data["text"], "Hello Liva")

    def test_transcribe_text_format(self):
        """Test transcription endpoint returning plain text response."""
        wav_bytes = create_mock_wav()
        response = self.client.post(
            "/v1/audio/transcriptions",
            files={"file": ("test.wav", wav_bytes, "audio/wav")},
            data={"response_format": "text"}
        )
        self.assertEqual(response.status_code, 200)
        self.assertEqual(response.text, "Hello Liva")

    def test_transcribe_srt_format(self):
        """Test transcription endpoint returning SRT response."""
        wav_bytes = create_mock_wav()
        response = self.client.post(
            "/v1/audio/transcriptions",
            files={"file": ("test.wav", wav_bytes, "audio/wav")},
            data={"response_format": "srt"}
        )
        self.assertEqual(response.status_code, 200)
        self.assertIn("1\n00:00:00,000 --> 00:00:05,000\nHello Liva", response.text)

    def test_transcribe_vtt_format(self):
        """Test transcription endpoint returning VTT response."""
        wav_bytes = create_mock_wav()
        response = self.client.post(
            "/v1/audio/transcriptions",
            files={"file": ("test.wav", wav_bytes, "audio/wav")},
            data={"response_format": "vtt"}
        )
        self.assertEqual(response.status_code, 200)
        self.assertIn("WEBVTT\n\n00:00:00.000 --> 00:00:05.000\nHello Liva", response.text)

    def test_transcribe_too_small_file(self):
        """Test transcription endpoint skips small file chunks (<1000 bytes)."""
        small_bytes = b"small_chunk"
        response = self.client.post(
            "/v1/audio/transcriptions",
            files={"file": ("test.wav", small_bytes, "audio/wav")}
        )
        self.assertEqual(response.status_code, 200)
        self.assertEqual(response.json()["text"], "")

    def test_device_detection_env_override(self):
        """Test get_device when WHISPER_DEVICE env var is explicitly configured."""
        with patch.dict(os.environ, {"WHISPER_DEVICE": "cuda"}):
            device = whisper_stt_server.get_device()
            self.assertEqual(device, "cuda")

        with patch.dict(os.environ, {"WHISPER_DEVICE": "cpu"}):
            device = whisper_stt_server.get_device()
            self.assertEqual(device, "cpu")

    def test_device_detection_no_pytorch(self):
        """Test get_device defaults to cpu when PyTorch is not installed."""
        with patch.dict(os.environ, {"WHISPER_DEVICE": "auto"}):
            with patch.dict(sys.modules, {"torch": None}):
                device = whisper_stt_server.get_device()
                self.assertEqual(device, "cpu")

    def test_device_detection_cuda_not_available(self):
        """Test get_device defaults to cpu when CUDA is not available."""
        mock_torch = MagicMock()
        mock_torch.cuda.is_available.return_value = False
        with patch.dict(os.environ, {"WHISPER_DEVICE": "auto"}):
            with patch.dict(sys.modules, {"torch": mock_torch}):
                device = whisper_stt_server.get_device()
                self.assertEqual(device, "cpu")

    def test_device_detection_cuda_available_and_usable(self):
        """Test get_device returns cuda when CUDA is available and usable."""
        mock_torch = MagicMock()
        mock_torch.cuda.is_available.return_value = True
        mock_torch.cuda.get_device_name.return_value = "NVIDIA RTX 4080"
        with patch.dict(os.environ, {"WHISPER_DEVICE": "auto"}):
            with patch.dict(sys.modules, {"torch": mock_torch}):
                device = whisper_stt_server.get_device()
                self.assertEqual(device, "cuda")

    def test_device_detection_cuda_available_but_unusable(self):
        """Test get_device falls back to cpu when CUDA is available but raises error (incompatible GPU)."""
        mock_torch = MagicMock()
        mock_torch.cuda.is_available.return_value = True
        mock_torch.cuda.get_device_name.side_effect = RuntimeError("CUDA error: device kernel image is invalid")
        with patch.dict(os.environ, {"WHISPER_DEVICE": "auto"}):
            with patch.dict(sys.modules, {"torch": mock_torch}):
                device = whisper_stt_server.get_device()
                self.assertEqual(device, "cpu")

if __name__ == '__main__':
    unittest.main()
