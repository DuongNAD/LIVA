import os
# Tối ưu hóa và chặn lỗi Triton trên Windows (giống file train)
os.environ["TORCHDYNAMO_DISABLE"] = "1"
os.environ["TORCH_COMPILE_DISABLE"] = "1"

import torch
from unsloth import FastLanguageModel
from transformers import TextStreamer

# ---------------------------------------------------------
# 1. CẤU HÌNH KIỂM THỬ
# ---------------------------------------------------------
max_seq_length = 2048 
dtype = None 
load_in_4bit = True 

# Đường dẫn trỏ tới Model LoRA bạn vừa huấn luyện xong
model_dir = "lora_model_high_quality"

print("Đang tải model (Fast Inference) từ:", model_dir)
try:
    model, tokenizer = FastLanguageModel.from_pretrained(
        model_name = model_dir, # Tải thẳng từ thư mục LoRA
        max_seq_length = max_seq_length,
        dtype = dtype,
        load_in_4bit = load_in_4bit,
    )
    # Kích hoạt chế độ suy luận siêu tốc (nhanh gấp 2 lần)
    FastLanguageModel.for_inference(model)
except Exception as e:
    print(f"Lỗi tải model. Đảm bảo model đã train xong! Lỗi: {e}")
    import sys
    sys.exit(1)

# ---------------------------------------------------------
# 2. DEFINITION TOOL (Giống chuẩn System Prompt)
# ---------------------------------------------------------
complex_tool = {
    "name": "schedule_events",
    "description": "Lên lịch các cuộc họp và sự kiện vào lịch của người dùng.",
    "parameters": {
        "type": "object",
        "properties": {
            "events": {
                "type": "array",
                "description": "Danh sách các sự kiện cần lên lịch",
                "items": {
                    "type": "object",
                    "properties": {
                        "title": {"type": "string", "description": "Tên sự kiện"},
                        "time": {"type": "string", "description": "Thời gian (VD: 14:00, Sáng mai)"},
                        "attendees": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Danh sách người tham gia (nếu có)"
                        }
                    },
                    "required": ["title", "time"]
                }
            }
        },
        "required": ["events"]
    }
}
import json
tools_json = f"""[
  {{
    "name": "get_system_time",
    "description": "Lấy thời gian hiện tại của hệ thống.",
    "parameters": {{"type": "object", "properties": {{}}, "required": []}}
  }},
  {{
    "name": "send_zalo_bot",
    "description": "Gửi tin nhắn qua Zalo Bot.",
    "parameters": {{
      "type": "object",
      "properties": {{
        "message": {{"type": "string", "description": "Nội dung tin nhắn cần gửi"}}
      }},
      "required": ["message"]
    }}
  }},
  {{
    "name": "read_emails",
    "description": "Đọc email từ hòm thư.",
    "parameters": {{
      "type": "object",
      "properties": {{
        "limit": {{"type": "number", "description": "Số lượng email cần đọc"}}
      }},
      "required": ["limit"]
    }}
  }},
  {{
    "name": "get_weather",
    "description": "Lấy thông tin thời tiết",
    "parameters": {{
      "type": "object",
      "properties": {{
        "location": {{"type": "string", "description": "Tên tỉnh/thành phố"}}
      }},
      "required": ["location"]
    }}
  }},
  {json.dumps(complex_tool, indent=2, ensure_ascii=False)}
]"""

base_sys_msg = f"""Bạn là Liva, một trợ lý AI thông minh.\n\nHƯỚNG DẪN DÙNG KỸ NĂNG:\nBạn có quyền truy cập vào các công cụ sau. Nếu yêu cầu cần dùng công cụ, hãy phản hồi bằng JSON gọi hàm. NẾU thiếu tham số, hãy hỏi lại người dùng. NẾU không có công cụ phù hợp, hãy từ chối.\n{tools_json}\n\nThời gian hiện tại của hệ thống là: 19:30:00 25/12/2026 (UTC+7)"""

# ---------------------------------------------------------
# 3. KỊCH BẢN KIỂM THỬ (CHƯA TỪNG CÓ TRONG DATA TRAIN)
# ---------------------------------------------------------
print("\n================================")
print("TEST CASE 1: Lên lịch phức tạp mảng lồng nhau chưa từng gặp")
print("Prompt: Lên lịch đi xem phim 'Spider-Man' ở CGV lúc 20:30 tối nay cùng với vợ anh và cu Bin. Xong nhắn Zalo báo mọi người chuẩn bị nhé.")

messages = [
    {"role": "system", "content": base_sys_msg},
    {"role": "user", "content": "Lên lịch đi xem phim 'Spider-Man' ở CGV lúc 20:30 tối nay cùng với vợ anh và cu Bin. Xong nhắn Zalo báo mọi người chuẩn bị nhé."}
]

inputs = tokenizer.apply_chat_template(
    messages,
    tokenize = True,
    add_generation_prompt = True, # Ép model sinh ra câu trả lời của trợ lý
    return_tensors = "pt",
).to("cuda")

# Sinh câu trả lời với luồng TextStreamer (để nhìn thấy ngay như ChatGPT)
text_streamer = TextStreamer(tokenizer, skip_prompt=True)

print("--- AI PHẢN HỒI ---")
_ = model.generate(input_ids = inputs, streamer = text_streamer, max_new_tokens = 256, use_cache = True)


print("\n================================")
print("TEST CASE 2: Chống Ảo Giác (Hỏi thời tiết nhưng không cấp địa điểm - Model phải hỏi lại)")
print("Prompt: Xem thời tiết tối nay có mưa không em?")

messages2 = [
    {"role": "system", "content": base_sys_msg},
    {"role": "user", "content": "Xem thời tiết tối nay có mưa không em?"}
]

inputs2 = tokenizer.apply_chat_template(messages2, tokenize = True, add_generation_prompt = True, return_tensors = "pt").to("cuda")
print("--- AI PHẢN HỒI ---")
_ = model.generate(input_ids = inputs2, streamer = text_streamer, max_new_tokens = 256, use_cache = True)


print("\n================================")
print("TEST CASE 3: Hỏi câu nằm ngoài hệ thống Tool (Yêu cầu bật đèn nhà)")
print("Prompt: Gọi tool mở đèn phòng khách cho anh.")

messages3 = [
    {"role": "system", "content": base_sys_msg},
    {"role": "user", "content": "Gọi tool mở đèn phòng khách cho anh."}
]

inputs3 = tokenizer.apply_chat_template(messages3, tokenize = True, add_generation_prompt = True, return_tensors = "pt").to("cuda")
print("--- AI PHẢN HỒI ---")
_ = model.generate(input_ids = inputs3, streamer = text_streamer, max_new_tokens = 256, use_cache = True)
