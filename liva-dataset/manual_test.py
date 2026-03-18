import os
os.environ["TORCHDYNAMO_DISABLE"] = "1"
os.environ["TORCH_COMPILE_DISABLE"] = "1"

import torch
import json
from unsloth import FastLanguageModel

max_seq_length = 2048
dtype = None
load_in_4bit = True
model_dir = "lora_model_v13_mastery"

print(f"Đang tải model {model_dir} để Test Manual...")
model, tokenizer = FastLanguageModel.from_pretrained(
    model_name=model_dir,
    max_seq_length=max_seq_length,
    dtype=dtype,
    load_in_4bit=load_in_4bit,
)
FastLanguageModel.for_inference(model)

tools_json = """[
  {
    "type": "function",
    "function": {
      "name": "get_system_time",
      "description": "Lấy thời gian hiện tại của hệ thống.",
      "parameters": {"type": "object", "properties": {}, "required": []}
    }
  },
  {
    "type": "function",
    "function": {
      "name": "send_zalo_bot",
      "description": "Gửi tin nhắn qua Zalo Bot. Lưu ý: Tóm tắt thông tin quan trọng trước khi gửi.",
      "parameters": {
        "type": "object",
        "properties": {
          "message": {"type": "string", "description": "Nội dung tin nhắn cần gửi"}
        },
        "required": ["message"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "read_emails",
      "description": "Đọc email từ hòm thư.",
      "parameters": {
        "type": "object",
        "properties": {
          "limit": {"type": "number", "description": "Số lượng email tối đa cần lấy"}
        },
        "required": ["limit"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "get_weather",
      "description": "Lấy thông tin thời tiết.",
      "parameters": {
        "type": "object",
        "properties": {
          "location": {"type": "string", "description": "Tên tỉnh/thành phố"}
        },
        "required": ["location"]
      }
    }
  }
]"""

base_sys_msg = f"Bạn là Liva, một trợ lý AI thông minh.\n\n# Tools\n\nYou may call one or more functions to assist with the user query.\n\nYou are provided with function signatures within <tools></tools> XML tags:\n<tools>\n{tools_json}\n</tools>\n\nFor each function call, return a json object with function name and arguments within <tool_call></tool_call> XML tags:\n<tool_call>\n{{\"name\": <function-name>, \"arguments\": <args-json-object>}}\n</tool_call>\n\nThời gian hệ thống hiện tại là: 10:00:00 01/01/2026 (UTC+7)."

custom_prompts = [
    # 1. Gọi chuẩn Zalo với thông tin rõ ràng
    "Liva ơi, nhắn tin zalo báo cáo tình hình dự án tuần này hoàn thành 100% KPI nhé.",
    
    # 2. Gọi 1 tool không tham số
    "Bạn cho mình biết bây giờ là mấy giờ rồi?",
    
    # 3. Yêu cầu thiếu tham số (Cần phải hỏi lại)
    "Nhắn Zalo giúp mình với.",
    
    # 4. Yêu cầu thời tiết thiếu địa điểm
    "Hôm nay trời có mưa không em?",
    
    # 5. Yêu cầu Zalo kết hợp tóm tắt (cần tóm tắt text dài)
    "Đọc hộ mình 3 email mới nhất, sau đó tóm tắt và nhắn zalo cho nhóm dev là 'đã duyệt bug' nhé.",
    
    # 6. Chitchat đánh lừa (có nhắc tên tool nhưng cấm gọi)
    "Theo em thì ứng dụng Zalo hay Messenger bảo mật tốt hơn?",
    
    # 7. Từ chối yêu cầu ngoài phạm vi
    "Em có thể tự động đặt vé máy bay đi Hà Nội ngày mai cho sếp được không?",
    
    # 8. Yêu cầu vừa đọc email vừa hỏi giờ (Nhiều ý trong 1 câu)
    "Mấy giờ rồi nhỉ? Tiện thể check 5 email mới nhất luôn nhé.",
    
    # 9. Đánh lừa hỏi thời tiết ở một nơi viễn tưởng
    "Thời tiết ở sao Hỏa hôm nay thế nào?",
    
    # 10. Chitchat trêu đùa
    "Nếu sếp trừ lương thì em có buồn không Liva?"
]

for i, prompt in enumerate(custom_prompts):
    print(f"\\n{'='*70}")
    print(f"🎯 TEST {i+1}: {prompt}")
    
    messages = [
        {"role": "system", "content": base_sys_msg},
        {"role": "user", "content": prompt}
    ]
    inputs = tokenizer.apply_chat_template(messages, tokenize=True, add_generation_prompt=True, return_tensors="pt").to("cuda")
    
    outputs = model.generate(input_ids=inputs, max_new_tokens=256, use_cache=True, pad_token_id=tokenizer.eos_token_id)
    output_text = tokenizer.decode(outputs[0][inputs.shape[1]:], skip_special_tokens=True).strip()
    
    print(f"🤖 LIVA PHẢN HỒI:\\n{output_text}")
    print(f"{'='*70}")
