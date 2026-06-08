import uvicorn
import os
import sys
from dotenv import load_dotenv
from llama_cpp.server.app import create_app  # type: ignore
from llama_cpp.server.settings import Settings  # type: ignore

# 1. Nạp biến môi trường từ Gateway (để biết đang dùng API hay Local)
base_dir = os.path.dirname(os.path.abspath(__file__))
env_path = os.path.join(os.path.dirname(base_dir), "liva-gateway", ".env")
load_dotenv(env_path)

# Constants
SEPARATOR = "=" * 50

if os.getenv("AI_PROVIDER") == "openai":
    print(SEPARATOR)
    print("☁️ [LIVA AI] Hệ thống đang chạy ở chế độ Cloud API (Gemini/OpenAI).")
    print(
        "💡 Engine cục bộ (Local Engine) không cần phải chạy. Vui lòng sử dụng Gateway!"
    )
    print(SEPARATOR)
    sys.exit(0)

from hardware_allocator import HardwareAllocator

# 2. Phát hiện phần cứng tối ưu
device = HardwareAllocator.get_optimal_device()
n_gpu_layers = -1 if device.type in ("mps", "cuda") else 0

# 3. Nếu là chế độ Local, mới nạp Model nặng vào
server_settings = Settings(
    model=os.path.join(os.getenv("AI_MODELS_DIR", "E:/AI_Models"), os.getenv("ROUTER_MODEL_NAME", "gemma-4-E2B-it-Q6_K.gguf")),
    n_gpu_layers=n_gpu_layers,
    n_ctx=8192,
    use_mmap=True,
    use_mlock=False,
    host="127.0.0.1",  # Chỉ cho phép truy cập cục bộ (Localhost)
    port=8000,  # Cổng giao tiếp với Gateway
)

# 4. Khởi tạo ứng dụng tương thích chuẩn OpenAI (OpenAI-compatible App)
app = create_app(settings=server_settings)

# 5. Kích hoạt động cơ (Start the Engine)
if __name__ == "__main__":
    print(SEPARATOR)
    print("🚀 [LIVA AI] Đang khởi động chế độ Cục Bộ (Local)")
    print(f"📂 Mô hình (Model): {server_settings.model}")
    print(SEPARATOR)

    uvicorn.run(app, host=server_settings.host, port=server_settings.port)
