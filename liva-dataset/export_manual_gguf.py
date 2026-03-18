import os
os.environ["TORCHDYNAMO_DISABLE"] = "1"
os.environ["TORCH_COMPILE_DISABLE"] = "1"

from unsloth import FastLanguageModel
import urllib.request
import json
import zipfile
import shutil
import sys

hf_dir = "E:/AI_Models/LIVA-Qwen2.5-7B-ToolCalling-HF"
gguf_f16 = "E:/AI_Models/LIVA-Qwen2.5-7B-ToolCalling-F16.gguf"
gguf_q8 = "E:/AI_Models/LIVA-Qwen2.5-7B-ToolCalling-unsloth.Q8_0.gguf"
llama_cpp_dir = "E:/AI_Models/llama_cpp_converter"

if not os.path.exists(hf_dir):
    print("1. Đang tải mô hình đã Fine-tune (LoRA)...")
    try:
        model, tokenizer = FastLanguageModel.from_pretrained(
            model_name = "outputs/checkpoint-250",
            max_seq_length = 2048,
            dtype = None,
            load_in_4bit = True,
        )
        print("2. Đang gộp trọng số và lưu dưới dạng HuggingFace 16-bit...")
        model.save_pretrained_merged(hf_dir, tokenizer, save_method = "merged_16bit")
    except Exception as e:
        print("Lỗi merge mô hình: ", e)
        sys.exit(1)
else:
    print("✅ Đã tìm thấy mô hình HuggingFace.")

if not os.path.exists(os.path.join(llama_cpp_dir, "convert_hf_to_gguf.py")):
    print("3. Kéo mã nguồn Llama.cpp để lấy script chuẩn hóa dữ liệu...")
    os.system(f"git clone https://github.com/ggerganov/llama.cpp.git {llama_cpp_dir}")
    os.system(f"pip install -r {llama_cpp_dir}/requirements.txt")

if not os.path.exists(gguf_f16):
    print("4. Chuyển đổi HF sang GGUF dạng F16...")
    os.system(f"py {llama_cpp_dir}/convert_hf_to_gguf.py {hf_dir} --outfile {gguf_f16} --outtype f16")
else:
    print("✅ Đã tìm thấy GGUF F16.")

print("5. Tải công cụ luồng Llama-quantize (đã biên dịch sẵn cho Windows)...")
quantize_exe = "E:/AI_Models/llama_bin/llama-quantize.exe"
if not os.path.exists(quantize_exe):
    os.makedirs("E:/AI_Models/llama_bin", exist_ok=True)
    req = urllib.request.Request("https://api.github.com/repos/ggerganov/llama.cpp/releases/latest")
    dl_url = None
    with urllib.request.urlopen(req) as response:
        data = json.loads(response.read())
        for a in data['assets']:
            # Tải bản vulkan x64 vì nó tích hợp sẵn CPU/GPU linh hoạt không cần CUDA Build Toolkit phức tạp
            if 'bin-win-vulkan-x64.zip' in a['name']:
                dl_url = a['browser_download_url']
                break
    
    if dl_url:
        print(f"Downloading: {dl_url}")
        urllib.request.urlretrieve(dl_url, "E:/AI_Models/llama_bin.zip")
        with zipfile.ZipFile("E:/AI_Models/llama_bin.zip", 'r') as zip_ref:
            zip_ref.extractall("E:/AI_Models/llama_bin")
    else:
        print("Không tìm thấy link tải llama-quantize hợp lệ.")

if os.path.exists(quantize_exe) and not os.path.exists(gguf_q8):
    print("6. Nén cực đại (Quantize Q8_0) cho LIVA Engine...")
    os.system(f"{quantize_exe} {gguf_f16} {gguf_q8} Q8_0")
    print("✅ LIVA Q8_0 GGUF đã sẵn sàng!")
    
    try:
        os.remove(gguf_f16)
        print("Đã xóa file F16 nháp để giải phóng 15GB ổ cứng.")
    except:
        pass
else:
    print(f"Bỏ qua Quantize do file {gguf_q8} đã tồn tại.")

print("\n🚀 TIẾN TRÌNH HOÀN THÀNH. MODEL LIVA ĐÃ ĐƯỢC TẢI XUỐNG E:/AI_Models!")
