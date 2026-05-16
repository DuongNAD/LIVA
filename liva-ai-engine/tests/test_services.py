"""Test LIVA Services Health"""
import socket
import sys

def check_port(host, port, name):
    """Check if a port is listening."""
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(2)
        result = sock.connect_ex((host, port))
        sock.close()
        if result == 0:
            print(f"[OK] {name} - Port {port} is OPEN")
            return True
        else:
            print(f"[FAIL] {name} - Port {port} is CLOSED")
            return False
    except Exception as e:
        print(f"[ERROR] {name} - Error: {e}")
        return False

def test_services():
    print("=" * 60)
    print("LIVA SYSTEM HEALTH CHECK")
    print("=" * 60)
    print()
    
    results = []
    
    print("[1/5] Testing Whisper STT Server (port 8101)...")
    results.append(check_port("127.0.0.1", 8101, "Whisper STT"))
    
    print()
    print("[2/5] Testing LLM Native Engine (port 8100)...")
    results.append(check_port("127.0.0.1", 8100, "LLM Engine"))
    
    print()
    print("[3/5] Testing Voice Engine (port 8002)...")
    results.append(check_port("127.0.0.1", 8002, "Voice Engine"))
    
    print()
    print("[4/5] Testing Gateway WebSocket (port 8082)...")
    results.append(check_port("127.0.0.1", 8082, "Gateway WS"))
    
    print()
    print("[5/5] Testing Vite Dev Server (port 5173)...")
    results.append(check_port("127.0.0.1", 5173, "Vite Dev"))
    
    print()
    print("=" * 60)
    print("SUMMARY")
    print("=" * 60)
    
    services = ["Whisper STT", "LLM Engine", "Voice Engine", "Gateway WS", "Vite Dev"]
    all_ok = True
    for i, service in enumerate(services):
        symbol = "[OK]" if results[i] else "[FAIL]"
        print(f"  {symbol} {service}")
        if not results[i]:
            all_ok = False
    
    print()
    if all_ok:
        print("SUCCESS: ALL SERVICES RUNNING!")
    else:
        print("WARNING: SOME SERVICES MISSING")
        print()
        print("To start missing services:")
        print("  1. Whisper STT: cd liva-ai-engine && python whisper_stt_server.py")
        print("  2. LLM Engine:  cd liva-ai-engine && python liva_native_engine.py")
        print("  3. Voice:       cd liva-ai-engine && python voice_engine.py")
        print("  4. Gateway:     cd openclaw-gateway && npm run dev")
    
    print()
    return all_ok

if __name__ == "__main__":
    success = test_services()
    sys.exit(0 if success else 1)
