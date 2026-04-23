from huggingface_hub import hf_hub_download
import os

repo_id = "unsloth/gemma-4-E2B-it-GGUF"
filename = "gemma-4-E2B-it-Q4_K_M.gguf"
local_dir = "E:\\AI_Models"

print(f"Downloading {filename} from {repo_id}...")
print(f"Local dir: {local_dir}")

try:
    hf_hub_download(
        repo_id=repo_id,
        filename=filename,
        local_dir=local_dir,
        local_dir_use_symlinks=False,
    )
    print("✅ Tải xong GGUF model!")
except Exception as e:
    print(f"❌ Lỗi tải: {e}")
