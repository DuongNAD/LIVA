import uvicorn
import os
import sys
import io
from dotenv import load_dotenv

# Buộc Terminal trên Windows (CP1252) phải hỗ trợ in Emoji 🧠 và tiếng Việt
if sys.stdout.encoding != 'utf-8':
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')
from llama_cpp.server.app import create_app  # type: ignore
from llama_cpp.server.settings import Settings  # type: ignore

# 1. Nạp biến môi trường
base_dir = os.path.dirname(os.path.abspath(__file__))
env_path = os.path.join(os.path.dirname(base_dir), "liva-gateway", ".env")
# Thêm override=True để luồn lách qua bộ nhớ đệm (cache) của hệ điều hành, ép Python đọc file .env gốc theo thời gian thực!
load_dotenv(env_path, override=True)

# Constants
SEPARATOR = "=" * 50

if os.getenv("AI_PROVIDER") == "openai":
    print(SEPARATOR)
    print("☁️ [LIVA AI Expert] Dang chay API Cloud (Gemini/OpenAI).")
    print(SEPARATOR)
    sys.exit(0)

import argparse
parser = argparse.ArgumentParser(description="LIVA AI Engine")
parser.add_argument("--role", type=str, default="coder", choices=["planner", "coder"], help="Vai trò của AI")
parser.add_argument("--port", type=int, default=8001, help="Cổng mạng")
parser.add_argument("--n_ctx", type=int, default=8192, help="Context Window")
args = parser.parse_args()

# 2. Định tuyến Bộ Phân tách Mô hình (Dynamic Model Routing)
models_dir = os.getenv("AI_MODELS_DIR", r"E:\AI_Models")
model_name = os.getenv("EXPERT_MODEL_NAME", "gemma-4-26B-A4B-it-UD-Q6_K.gguf")

if not model_name:
    print("❌ [LỖI] Không tìm thấy khóa EXPERT_MODEL_NAME trong file .env!")
    sys.exit(1)

server_settings = Settings(
    model=os.path.join(models_dir, model_name),
    n_gpu_layers=-1,      # Rút MẠNH xuống 32 lớp (Giữ lại hẳn 3GB VRAM rỗng) để KHÔNG BAO GIỜ bị CUDA Graph Deadlock nữa!
    n_ctx=args.n_ctx,     # Context linh động qua CLI
    n_batch=512,          # Đưa về 512 mặc định để VRAM có không gian thở cực đại
    n_threads=4,          # Giảm luồng CPU tránh xung đột Wait-State
    use_mmap=True,        # Bật Memory-Mapped File để OS tự động paging những lớp bị tràn ra khỏi VRAM (tiết kiệm ~15GB RAM).
    use_mlock=False,      # Bỏ khóa RAM tiến trình. Giải phóng hoàn toàn bộ nhớ Committed bị phình to.
    flash_attn=True,      # Bắt buộc bật Flash Attention để chống nổ VRAM với tham số 26B
    type_k=8, type_v=8,   # Nén KV Cache 8-bit
    host="127.0.0.1",     
    port=args.port,       # Cổng linh động qua CLI
    chat_format="qwen",   # Sử dụng Chat template của Qwen
)

# 3. Khởi tạo ứng dụng tương thích chuẩn OpenAI
app = create_app(settings=server_settings)

# 4. Kích hoạt động cơ
if __name__ == "__main__":
    print(SEPARATOR)
    print(f"🧠 [LIVA AI {args.role.upper()}] Đã kích hoạt Siêu Não (Cổng {args.port})")
    print(f"📂 Mô hình (Expert Model): {server_settings.model}")
    print(f"📏 Context Window đang sử dụng: args={args.n_ctx} | settings={server_settings.n_ctx}")
    print(SEPARATOR)

    # Uvicorn config for graceful shutdown
    config = uvicorn.Config(app, host=server_settings.host, port=server_settings.port, loop="asyncio")
    server = uvicorn.Server(config)
    server.run()
