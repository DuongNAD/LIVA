from llama_cpp.server.app import create_app, Settings
from fastapi.testclient import TestClient
import traceback
import sys
import os
from dotenv import load_dotenv

# Kiểm tra xem có đang dùng Gateway API Cloud không
base_dir = os.path.dirname(os.path.abspath(__file__))
env_path = os.path.join(os.path.dirname(base_dir), "openclaw-gateway", ".env")
load_dotenv(env_path)

if os.getenv("AI_PROVIDER") == "openai":
    print("==================================================")
    print("☁️ [LIVA AI] Hệ thống đang chạy ở chế độ Cloud API (Gemini/OpenAI).")
    print(
        "💡 Bản thử nghiệm (Debug Script) này chỉ dùng cho Local Model. Vui lòng chat trực tiếp trên UI Liva!"
    )
    print("==================================================")
    sys.exit(0)

try:
    settings = Settings(
        model="E:/AI_Models/Qwen2.5-7B-Instruct-Q8_0.gguf", n_gpu_layers=-1
    )
    app = create_app(settings)
    client = TestClient(app, raise_server_exceptions=True)

    response = client.post(
        "/v1/chat/completions",
        json={
            "model": "qwen",
            "messages": [
                {
                    "role": "system",
                    "content": "Bạn là Liva, một AI thông minh, tinh tế và duyên dáng. Bạn CHỈ ĐƯỢC PHÉP trả lời bằng tiếng Việt, tuyệt đối không sử dụng ngôn ngữ khác. Hãy trả lời ngắn gọn, tự nhiên.",
                },
                {"role": "user", "content": "hi"},
            ],
            "temperature": 0.3,
        },
    )
    print(response.json())
except Exception as e:
    traceback.print_exc()
    sys.exit(1)
