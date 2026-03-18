import os
os.environ["TORCHDYNAMO_DISABLE"] = "1"
os.environ["TORCH_COMPILE_DISABLE"] = "1"

from unsloth import FastLanguageModel

print("1. Đang tải mô hình đã Fine-tune (LoRA)...")
model, tokenizer = FastLanguageModel.from_pretrained(
    model_name = "lora_model_v13_mastery",
    max_seq_length = 2048,
    dtype = None,
    load_in_4bit = True,
)

# Thư mục đích lưu trữ GGUF
export_dir = "E:/AI_Models"
os.makedirs(export_dir, exist_ok=True)
model_prefix = "LIVA-Qwen2.5-7B-ToolCalling"

print(f"2. Bắt đầu gộp trọng số (Merge Weights) và xuất ra định dạng GGUF (Q8_0) tại {export_dir}...")
# Unsloth sẽ tự động gộp base model + LoRA và gọi thư viện llama.cpp để biên dịch thành file 1 cục.
model.save_pretrained_gguf(os.path.join(export_dir, model_prefix), tokenizer, quantization_method = "q8_0")

print("=================================================")
print(f"✅ Hoàn tất! Model đã được lưu dưới dạng GGUF trong thư mục {export_dir}")
print("Tên file gốc thường sẽ có dạng: LIVA-Qwen2.5-7B-ToolCalling-unsloth.Q8_0.gguf")
print("=================================================")
