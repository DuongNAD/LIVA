"""Test LIVA Services Health"""
import unittest
import socket

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

class TestServicesHealth(unittest.TestCase):
    def test_whisper_stt(self):
        """Test Whisper STT Server (port 8101)"""
        self.assertTrue(check_port("127.0.0.1", 8101, "Whisper STT"), "Whisper STT - Port 8101 is CLOSED")
    
    def test_llm_engine(self):
        """Test LLM Native Engine (port 8100)"""
        self.assertTrue(check_port("127.0.0.1", 8100, "LLM Engine"), "LLM Engine - Port 8100 is CLOSED")

    def test_voice_engine(self):
        """Test Voice Engine (port 8002)"""
        self.assertTrue(check_port("127.0.0.1", 8002, "Voice Engine"), "Voice Engine - Port 8002 is CLOSED")

    def test_gateway_ws(self):
        """Test Gateway WebSocket (port 8082)"""
        self.assertTrue(check_port("127.0.0.1", 8082, "Gateway WS"), "Gateway WS - Port 8082 is CLOSED")

    @unittest.skip("Vite dev server is only checked in full E2E environments")
    def test_vite_dev(self):
        """Test Vite Dev Server (port 5173)"""
        self.assertTrue(check_port("127.0.0.1", 5173, "Vite Dev"), "Vite Dev - Port 5173 is CLOSED")

if __name__ == "__main__":
    unittest.main()
