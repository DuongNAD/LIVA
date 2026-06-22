import unittest
import json
import base64
from unittest.mock import patch, MagicMock, AsyncMock
from fastapi.testclient import TestClient

import sys
import os
sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from voice_engine import app, sanitize_for_tts, ALLOWED_VOICES

# Mock Communicate
class MockCommunicate:
    def __init__(self, text, voice, rate=None):
        self.text = text
        self.voice = voice

    async def stream(self):
        # Simulate edge_tts chunk stream
        yield {"type": "audio", "data": b"mock_audio_" + self.text.encode('utf-8')}

class TestVoiceEngine(unittest.TestCase):
    def setUp(self):
        self.client = TestClient(app)

    def test_sanitize_text(self):
        # Test markdown Bold
        self.assertEqual(sanitize_for_tts("**hello**"), "hello")
        # Test Emojis
        self.assertEqual(sanitize_for_tts("hello 😊"), "hello")
        # Test Code Blocks
        self.assertEqual(sanitize_for_tts("```python\nprint('hi')\n``` hello"), "hello")
        # Test URL
        self.assertEqual(sanitize_for_tts("click here https://example.com/foo"), "click here")
        # Test Angle brackets
        self.assertEqual(sanitize_for_tts("<hello>"), "hello")

    @patch('edge_tts.Communicate', MockCommunicate)
    def test_tts_http_endpoint(self):
        response = self.client.post("/tts", json={"text": "hello test"})
        self.assertEqual(response.status_code, 200)
        data = response.json()
        self.assertEqual(data["status"], "ok")
        audio_bytes = base64.b64decode(data["audio"])
        self.assertTrue(audio_bytes.startswith(b"mock_audio_hello test"))

    @patch('edge_tts.Communicate', MockCommunicate)
    def test_tts_http_empty(self):
        # Emojis only should be sanitized to empty and return status empty
        response = self.client.post("/tts", json={"text": "😊😊😊"})
        self.assertEqual(response.status_code, 200)
        data = response.json()
        self.assertEqual(data["status"], "empty")

    @patch('edge_tts.Communicate', MockCommunicate)
    def test_ws_ping(self):
        with self.client.websocket_connect("/ws") as websocket:
            websocket.send_text(json.dumps({"type": "ping"}))
            response = websocket.receive_text()
            data = json.loads(response)
            self.assertEqual(data["type"], "pong")

    @patch('edge_tts.Communicate', MockCommunicate)
    def test_ws_set_voice(self):
        with self.client.websocket_connect("/ws") as websocket:
            websocket.send_text(json.dumps({"type": "set_voice", "voice": "en-US-AvaMultilingualNeural"}))
            websocket.send_text(json.dumps({"type": "set_voice", "voice": "invalid_voice"}))
            websocket.send_text(json.dumps({"type": "ping"}))
            response = websocket.receive_text()
            data = json.loads(response)
            self.assertEqual(data["type"], "pong")

    @patch('edge_tts.Communicate', MockCommunicate)
    def test_ws_tts_stream(self):
        with self.client.websocket_connect("/ws") as websocket:
            websocket.send_text(json.dumps({"type": "tts", "text": "hello world"}))
            response = websocket.receive_text()
            data = json.loads(response)
            self.assertEqual(data["type"], "audio")
            audio_bytes = base64.b64decode(data["data"])
            self.assertEqual(audio_bytes, b"mock_audio_hello world")

    @patch('edge_tts.Communicate', MockCommunicate)
    def test_ws_malformed_json(self):
        with self.client.websocket_connect("/ws") as websocket:
            websocket.send_text("not a json string")
            websocket.send_text(json.dumps({"type": "ping"}))
            response = websocket.receive_text()
            data = json.loads(response)
            self.assertEqual(data["type"], "pong")

    @patch('edge_tts.Communicate', MockCommunicate)
    def test_ws_interrupt(self):
        with self.client.websocket_connect("/ws") as websocket:
            websocket.send_text(json.dumps({"type": "interrupt"}))
            websocket.send_text(json.dumps({"type": "ping"}))
            response = websocket.receive_text()
            data = json.loads(response)
            self.assertEqual(data["type"], "pong")

if __name__ == '__main__':
    unittest.main()
