from llama_cpp.server.app import create_app, Settings
from fastapi.testclient import TestClient
import traceback
import sys

try:
    settings = Settings(model="E:/AI_Models/Qwen2.5-7B-Instruct-Q8_0.gguf", n_gpu_layers=-1)
    app = create_app(settings)
    client = TestClient(app, raise_server_exceptions=True)
    
    response = client.post("/v1/chat/completions", json={
        "model": "qwen",
        "messages": [{"role": "user", "content": "hi"}]
    })
    print(response.json())
except Exception as e:
    traceback.print_exc()
    sys.exit(1)
