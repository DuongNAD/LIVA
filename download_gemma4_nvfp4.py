from huggingface_hub import snapshot_download

model_id = "bg-digitalservices/Gemma-4-26B-A4B-it-NVFP4"
local_dir = r"E:\AI_Models\Gemma-4-26B-A4B-it-NVFP4"

print(f"Downloading model {model_id} NVFP4...")
print(f"Local dir: {local_dir}")

try:
    snapshot_download(repo_id=model_id, local_dir=local_dir, repo_type="model", max_workers=8)
    print("Finished downloading!")
except Exception as e:
    print(f"Error: {e}")
