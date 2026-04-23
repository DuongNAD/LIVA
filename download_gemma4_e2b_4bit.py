from huggingface_hub import snapshot_download

# Thay đổi repo_id theo đúng phiên bản 4-bit bạn muốn dùng (Ví dụ unsloth/..., GGUF, AWQ, GPTQ...)
model_id = "unsloth/gemma-4-E2B-it-unsloth-bnb-4bit"
local_dir = r"E:\AI_Models\Gemma-4-E2B-it-4bit"

print(f"Downloading model {model_id} 4-bit...")
print(f"Local dir: {local_dir}")

try:
    # max_workers giúp tải song song nhiều file, resume_download giúp tải tiếp nếu mạng bị lỗi
    snapshot_download(
        repo_id=model_id, 
        local_dir=local_dir, 
        repo_type="model", 
        max_workers=8,
        resume_download=True
    )
    print("Finished downloading!")
except Exception as e:
    print(f"Error: {e}")
