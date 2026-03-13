import uvicorn
from llama_cpp.server.app import create_app
from llama_cpp.server.settings import Settings

server_settings = Settings(
    model="E:/AI_Models/Qwen2.5-7B-Instruct-Q8_0.gguf", # Đường dẫn tới mô hình (Model path)
    n_gpu_layers=-1,                        # Offload 100% các lớp tính toán lên VRAM RTX 5060 Ti
    n_ctx=4096,                             # Mở rộng cửa sổ ngữ cảnh (Context Window)
    host="127.0.0.1",                       # Chỉ cho phép truy cập cục bộ (Localhost)
    port=8000                               # Cổng giao tiếp với Gateway
)

# 2. Khởi tạo ứng dụng tương thích chuẩn OpenAI (OpenAI-compatible App)
app = create_app(settings=server_settings)

# 3. Kích hoạt động cơ (Start the Engine)
if __name__ == "__main__":
    print("==================================================")
    print("🚀 [LIVA AI] Đang khởi động")
    print(f"📂 Mô hình (Model): {server_settings.model}")
    print("==================================================")
    
    uvicorn.run(
        app, 
        host=server_settings.host, 
        port=server_settings.port
    )