import os
from huggingface_hub import hf_hub_download

repo_id = 'Qwen/Qwen2.5-Coder-7B-Instruct-GGUF'
filename = 'qwen2.5-coder-7b-instruct-q5_k_m.gguf'
target_dir = r'E:\Project\LIVA\liva-ai-engine\models'

print(f'Starting download of {filename} from {repo_id} to {target_dir}...')
os.makedirs(target_dir, exist_ok=True)

try:
    downloaded_path = hf_hub_download(repo_id=repo_id, filename=filename, local_dir=target_dir)
    print(f'Download complete! Saved to {downloaded_path}')
except Exception as e:
    print(f'Failed to download: {e}')
