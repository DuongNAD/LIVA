import os
os.environ["TORCHDYNAMO_DISABLE"] = "1"
os.environ["TORCH_COMPILE_DISABLE"] = "1"
os.environ["PYTORCH_CUDA_ALLOC_CONF"] = "expandable_segments:True" # Tối ưu hóa phân mảnh VRAM trên Windows
os.environ["UNSLOTH_CE_LOSS_TARGET_GB"] = "2" # Bỏ qua check VRAM ảo của Unsloth gây ra lỗi Fused Cross Entropy
import torch
from datasets import load_dataset
from unsloth import FastLanguageModel
from unsloth.chat_templates import get_chat_template, standardize_sharegpt
from trl import SFTTrainer
from transformers import TrainingArguments
import sys

# ---------------------------------------------------------
# 1. PARAMETERS & CONFIGURATION (MAX QUALITY)
# ---------------------------------------------------------
max_seq_length = 1024 # Trả về 1024 vì Dataset thực tế dài tới 577 tokens
dtype = None 
load_in_4bit = True 

dataset_file = "train_zalo_tool.jsonl"
valid_file = "validation_zalo_tool.jsonl"
model_name = "Qwen/Qwen2.5-7B-Instruct" 
output_dir = "lora_model_v13_mastery" 

print(f"Bắt đầu quy trình Fine-tuning Chất Lượng Cao {model_name}...")

# ---------------------------------------------------------
# 2. LOAD MODEL & TOKENIZER
# ---------------------------------------------------------
try:
    model, tokenizer = FastLanguageModel.from_pretrained(
        model_name = model_name,
        max_seq_length = max_seq_length,
        dtype = dtype,
        load_in_4bit = load_in_4bit,
    )

    # Cấu hình LoRA sâu hơn để dứt điểm lỗi bỏ qua tool chỉ định Zalo
    model = FastLanguageModel.get_peft_model(
        model,
        r = 16, # Rank 16 thay vì 64 để nhét vừa 16GB VRAM khi đã bật All_linear
        target_modules = ["q_proj", "k_proj", "v_proj", "o_proj",
                          "gate_proj", "up_proj", "down_proj",], # Thêm MLP (Gate/Up/Down) để tăng cường Logic Tool Calling
        lora_alpha = 16,
        lora_dropout = 0.05,
        bias = "none",
        use_gradient_checkpointing = "unsloth", # Dùng Unsloth Checkpointing (bắt buộc để tiết kiệm 30% VRAM)
        random_state = 3407,
        use_rslora = True, 
        loftq_config = None,
    )
except Exception as e:
    print(f"LỖI TẢI MODEL: {e}")
    sys.exit(1)

# ---------------------------------------------------------
# 3. FORMAT DATASET
# ---------------------------------------------------------
# Qwen2.5 dùng ChatML và hỗ trợ natively function calling qua apply_chat_template

def formatting_prompts_func(examples):
    texts = [tokenizer.apply_chat_template(messages, tokenize = False, add_generation_prompt = False) for messages in examples["messages"]]
    return { "text" : texts, }

try:
    dataset = load_dataset("json", data_files=dataset_file, split="train")
    dataset = dataset.map(formatting_prompts_func, batched = True,)
    
    valid_dataset = load_dataset("json", data_files=valid_file, split="train")
    valid_dataset = valid_dataset.map(formatting_prompts_func, batched = True,)
except Exception as e:
    print(f"LỖI TẢI DATASET: {e}")
    sys.exit(1)

# ---------------------------------------------------------
# 4. START TRAINING
# ---------------------------------------------------------
trainer = SFTTrainer(
    model = model,
    tokenizer = tokenizer,
    train_dataset = dataset,
    eval_dataset = valid_dataset,
    dataset_text_field = "text",
    max_seq_length = max_seq_length,
    dataset_num_proc = 2,
    packing = False,
    args = TrainingArguments(
        per_device_train_batch_size = 1, # Hạ batch size xuống 1 để tránh OOM trên 16GB VRAM
        gradient_accumulation_steps = 8, # Tổng batch size là 1 * 8 = 8 (giữ nguyên chất lượng)
        warmup_ratio = 0.1, # Warmup 10% quá trình học
        num_train_epochs = 4, # Học kỹ toàn bộ data 4 lần
        learning_rate = 5e-5, # Tốc độ học nhỏ để đảm bảo chất lượng, tránh quên lệch
        fp16 = not torch.cuda.is_bf16_supported(),
        bf16 = torch.cuda.is_bf16_supported(),
        logging_steps = 5, # In log mỗi 5 steps để dễ theo dõi
        optim = "adamw_8bit",
        weight_decay = 0.05,
        lr_scheduler_type = "cosine", # Hạ từ từ LR ở cuối quá trình tăng độ hội tụ
        seed = 3407,
        output_dir = "outputs",
        report_to = "none",
        eval_strategy = "steps",
        eval_steps = 10,
        save_strategy = "epoch", # Lưu model sau mỗi epoch để dự phòng
    ),
)

print("Đang khởi động Training...")
try:
    trainer_stats = trainer.train()
except Exception as e:
    print(f"LỖI HUẤN LUYỆN: {e}")
    sys.exit(1)

# ---------------------------------------------------------
# 5. SAVE MODEL
# ---------------------------------------------------------
try:
    model.save_pretrained(output_dir)
    tokenizer.save_pretrained(output_dir)
    print(f"\n==============================================")
    print(f"🎉 Huấn luyện hoàn tất! Model lưu tại: {output_dir}")
    print(f"Loss File: {trainer_stats.metrics}")
    print(f"==============================================")
except Exception as e:
    print(f"LỖI LƯU MODEL: {e}")
