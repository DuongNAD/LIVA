import importlib.util
import io
import json
import tempfile
import unittest
import wave
from pathlib import Path


SCRIPT = Path(__file__).with_name("prepare-fleurs-vi.py")
SPEC = importlib.util.spec_from_file_location("prepare_fleurs_vi", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class PrepareFleursViTests(unittest.TestCase):
    def test_writes_pcm16_wav_and_relative_jsonl_manifest(self):
        rows = [
            {
                "audio": {"array": [0.0, 0.5, -0.5], "sampling_rate": 16000},
                "transcription": "xin chào",
            }
        ]
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp)
            manifest = MODULE.materialize(rows, output, 1)
            records = [json.loads(line) for line in manifest.read_text(encoding="utf-8").splitlines()]
            self.assertEqual(records, [{"audio": "audio/0000.wav", "transcript": "xin chào"}])
            with wave.open(str(output / "audio" / "0000.wav"), "rb") as wav:
                self.assertEqual(wav.getframerate(), 16000)
                self.assertEqual(wav.getnchannels(), 1)
                self.assertEqual(wav.getsampwidth(), 2)
                self.assertEqual(wav.getnframes(), 3)

    def test_accepts_undecoded_wav_bytes_without_torchcodec(self):
        source = io.BytesIO()
        with wave.open(source, "wb") as wav:
            wav.setnchannels(1)
            wav.setsampwidth(2)
            wav.setframerate(16000)
            wav.writeframes(b"\x00\x00\xff\x7f")
        rows = [{"audio": {"bytes": source.getvalue(), "path": "sample.wav"}, "transcription": "hai mẫu"}]
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp)
            MODULE.materialize(rows, output, 1)
            with wave.open(str(output / "audio" / "0000.wav"), "rb") as wav:
                self.assertEqual(wav.readframes(2), b"\x00\x00\xff\x7f")


if __name__ == "__main__":
    unittest.main()
