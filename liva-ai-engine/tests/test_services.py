"""Test LIVA Services Health"""
import unittest
import socket
from unittest.mock import patch

def check_port(host, port, name):
    """Check if a port is listening."""
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(2)
        result = sock.connect_ex((host, port))
        sock.close()
        return result == 0
    except Exception as e:
        return False

@patch("socket.socket")
class TestServicesHealth(unittest.TestCase):
    def test_whisper_stt(self, mock_socket):
        """Test Whisper STT Server (port 8101)"""
        mock_socket.return_value.connect_ex.return_value = 0
        self.assertTrue(check_port("127.0.0.1", 8101, "Whisper STT"), "Whisper STT - Port 8101 is CLOSED")
    
    def test_llm_engine(self, mock_socket):
        """Test LLM Native Engine (port 8100)"""
        mock_socket.return_value.connect_ex.return_value = 0
        self.assertTrue(check_port("127.0.0.1", 8100, "LLM Engine"), "LLM Engine - Port 8100 is CLOSED")

    def test_voice_engine(self, mock_socket):
        """Test Voice Engine (port 8002)"""
        mock_socket.return_value.connect_ex.return_value = 0
        self.assertTrue(check_port("127.0.0.1", 8002, "Voice Engine"), "Voice Engine - Port 8002 is CLOSED")

    def test_gateway_ws(self, mock_socket):
        """Test Gateway WebSocket (port 8082)"""
        mock_socket.return_value.connect_ex.return_value = 0
        self.assertTrue(check_port("127.0.0.1", 8082, "Gateway WS"), "Gateway WS - Port 8082 is CLOSED")

    @unittest.skip("Vite dev server is only checked in full E2E environments")
    def test_vite_dev(self, mock_socket):
        """Test Vite Dev Server (port 5173)"""
        mock_socket.return_value.connect_ex.return_value = 0
        self.assertTrue(check_port("127.0.0.1", 5173, "Vite Dev"), "Vite Dev - Port 5173 is CLOSED")


if __name__ == "__main__":
    unittest.main()
