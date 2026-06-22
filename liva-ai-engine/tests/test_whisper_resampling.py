"""Unit tests for Whisper STT audio resampling and downmixing."""
import unittest
import io
import wave
import numpy as np
from unittest.mock import MagicMock, patch, AsyncMock

# Ensure the root directory is in sys.path
import sys
import os
sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import whisper_stt_server

def create_wave_bytes(frequency=440.0, sample_rate=16000, duration_seconds=0.5, channels=1, sampwidth=2):
    num_samples = int(sample_rate * duration_seconds)
    t = np.linspace(0, duration_seconds, num_samples, endpoint=False)
    
    if channels == 1:
        data = np.sin(2 * np.pi * frequency * t)
    else:
        data = np.zeros((num_samples, channels))
        for c in range(channels):
            data[:, c] = np.sin(2 * np.pi * (frequency + c * 100) * t)
        data = data.flatten()
        
    if sampwidth == 2:
        int_data = (data * 32767.0).astype(np.int16)
        binary_data = int_data.tobytes()
    elif sampwidth == 4:
        float_data = data.astype(np.float32)
        binary_data = float_data.tobytes()
    else:
        raise ValueError("Unsupported sample width")
        
    out_io = io.BytesIO()
    with wave.open(out_io, 'wb') as wav:
        wav.setnchannels(channels)
        wav.setsampwidth(sampwidth)
        wav.setframerate(sample_rate)
        wav.writeframes(binary_data)
        
    return out_io.getvalue()

class TestWhisperResampling(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        # Create a mock Whisper model
        self.mock_model = MagicMock()
        # Mock the transcribe method
        # It needs to return a tuple of (segments, info)
        # segments is an iterable of Segment objects. We can return an empty list or mock segments
        mock_segment = MagicMock()
        mock_segment.text = "Hello world"
        self.mock_model.transcribe.return_value = ([mock_segment], MagicMock())
        
        # Patch the global model and model_loaded in whisper_stt_server
        self.model_patcher = patch('whisper_stt_server.model', self.mock_model)
        self.model_loaded_patcher = patch('whisper_stt_server.model_loaded', True)
        self.model_patcher.start()
        self.model_loaded_patcher.start()

    async def asyncTearDown(self):
        self.model_patcher.stop()
        self.model_loaded_patcher.stop()

    async def test_resample_16khz_mono(self):
        """16kHz mono WAV should not be resampled and should pass through directly."""
        wav_bytes = create_wave_bytes(sample_rate=16000, duration_seconds=0.5, channels=1)
        
        text = await whisper_stt_server.transcribe_audio(wav_bytes)
        
        # Verify transcribe was called
        self.mock_model.transcribe.assert_called_once()
        args, kwargs = self.mock_model.transcribe.call_args
        audio_array = args[0]
        
        # 0.5s at 16kHz is 8000 samples
        self.assertEqual(len(audio_array), 8000)
        self.assertEqual(audio_array.ndim, 1)

    async def test_resample_48khz_mono(self):
        """48kHz mono WAV should be resampled to 16kHz."""
        wav_bytes = create_wave_bytes(sample_rate=48000, duration_seconds=0.5, channels=1)
        
        text = await whisper_stt_server.transcribe_audio(wav_bytes)
        
        self.mock_model.transcribe.assert_called_once()
        args, kwargs = self.mock_model.transcribe.call_args
        audio_array = args[0]
        
        # 0.5s at 16kHz should be 8000 samples
        # Let's verify that the resampled length is approximately 8000 (allow minor filtering edge effects if any)
        self.assertTrue(7900 <= len(audio_array) <= 8100, f"Expected ~8000 samples, got {len(audio_array)}")
        self.assertEqual(audio_array.ndim, 1)

    async def test_resample_44_1khz_stereo(self):
        """44.1kHz stereo WAV should be downmixed to mono and resampled to 16kHz."""
        wav_bytes = create_wave_bytes(sample_rate=44100, duration_seconds=0.5, channels=2)
        
        text = await whisper_stt_server.transcribe_audio(wav_bytes)
        
        self.mock_model.transcribe.assert_called_once()
        args, kwargs = self.mock_model.transcribe.call_args
        audio_array = args[0]
        
        # 0.5s at 16kHz should be 8000 samples
        self.assertTrue(7900 <= len(audio_array) <= 8100, f"Expected ~8000 samples, got {len(audio_array)}")
        self.assertEqual(audio_array.ndim, 1)

if __name__ == '__main__':
    unittest.main()
