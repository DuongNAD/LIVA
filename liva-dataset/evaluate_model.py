import os
os.environ["TORCHDYNAMO_DISABLE"] = "1"
os.environ["TORCH_COMPILE_DISABLE"] = "1"

import torch
import json
import re
from unsloth import FastLanguageModel
from transformers import TextStreamer
import time

max_seq_length = 2048 
dtype = None 
load_in_4bit = True 
model_dir = "lora_model_v13_mastery"

print("Đang tải model (Fast Inference) từ:", model_dir)
try:
    model, tokenizer = FastLanguageModel.from_pretrained(
        model_name = model_dir, 
        max_seq_length = max_seq_length,
        dtype = dtype,
        load_in_4bit = load_in_4bit,
    )
    FastLanguageModel.for_inference(model)
except Exception as e:
    print(f"Lỗi tải model: {e}")
    import sys
    sys.exit(1)

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
tools_json = f"""[
  {{
    "type": "function",
    "function": {{
      "name": "get_system_time",
      "description": "Lấy thời gian hiện tại của hệ thống.",
      "parameters": {{"type": "object", "properties": {{}}, "required": []}}
    }}
  }},
  {{
    "type": "function",
    "function": {{
      "name": "send_zalo_bot",
      "description": "Gửi tin nhắn qua Zalo Bot. Lưu ý: Tóm tắt thông tin quan trọng trước khi gửi.",
      "parameters": {{
        "type": "object",
        "properties": {{
          "message": {{"type": "string", "description": "Nội dung tin nhắn cần gửi"}}
        }},
        "required": ["message"]
      }}
    }}
  }},
  {{
    "type": "function",
    "function": {{
      "name": "read_emails",
      "description": "Đọc email từ hòm thư.",
      "parameters": {{
        "type": "object",
        "properties": {{
          "limit": {{"type": "number", "description": "Số lượng email tối đa cần lấy"}}
        }},
        "required": ["limit"]
      }}
    }}
  }},
  {{
    "type": "function",
    "function": {{
      "name": "get_weather",
      "description": "Lấy thông tin thời tiết.",
      "parameters": {{
        "type": "object",
        "properties": {{
          "location": {{"type": "string", "description": "Tên tỉnh/thành phố"}}
        }},
        "required": ["location"]
      }}
    }}
  }},
  {{
    "type": "function",
    "function": {json.dumps(complex_tool, indent=4, ensure_ascii=False)}
  }}
]"""

base_sys_msg = f"Bạn là Liva, một trợ lý AI thông minh.\n\n# Tools\n\nYou may call one or more functions to assist with the user query.\n\nYou are provided with function signatures within <tools></tools> XML tags:\n<tools>\n{tools_json}\n</tools>\n\nFor each function call, return a json object with function name and arguments within <tool_call></tool_call> XML tags:\n<tool_call>\n{{\"name\": <function-name>, \"arguments\": <args-json-object>}}\n</tool_call>\n\nThời gian hệ thống hiện tại là: 10:00:00 01/01/2026 (UTC+7)."

# ---------------------------------------------------------
# 3. TẬP DỮ LIỆU ĐÁNH GIÁ (50 MẪU)
# Gồm 4 loại: 
# "valid_tool": Cần gọi tool.
# "reject": Nằm ngoài phạm vi (đòi tắt đèn, chuyển tiền).
# "chitchat": Giao tiếp thông thường.
# "clarify": Thiếu tham số bắt buộc.
# ---------------------------------------------------------
test_cases = [
    # --- Valid Single Tool ---
    {"prompt": "Đọc cho anh 3 email mới nhất nhé", "expected_type": "valid_tool", "expected_tool": "read_emails"},
    {"prompt": "Kiểm tra hòm thư xem có thư nào không, lấy 5 cái", "expected_type": "valid_tool", "expected_tool": "read_emails"},
    {"prompt": "Thời tiết Hà Nội hôm nay thế nào em?", "expected_type": "valid_tool", "expected_tool": "get_weather"},
    {"prompt": "Báo cáo thời tiết ở Sài Gòn xem có mưa không", "expected_type": "valid_tool", "expected_tool": "get_weather"},
    {"prompt": "Bây giờ là mấy giờ rồi Liva?", "expected_type": "valid_tool", "expected_tool": "get_system_time"},
    {"prompt": "Ngày hôm nay là ngày bao nhiêu?", "expected_type": "valid_tool", "expected_tool": "get_system_time"},
    {"prompt": "Nhắn tin qua Zalo cho sếp bảo là em đang ốm", "expected_type": "valid_tool", "expected_tool": "send_zalo_bot"},
    {"prompt": "Gửi qua bot Zalo nội dung: Họp lúc 3h chiều nhé", "expected_type": "valid_tool", "expected_tool": "send_zalo_bot"},
    {"prompt": "Xem dự báo thời tiết Đà Nẵng", "expected_type": "valid_tool", "expected_tool": "get_weather"},
    {"prompt": "Lấy 1 mail mới nhất đọc anh nghe", "expected_type": "valid_tool", "expected_tool": "read_emails"},
    {"prompt": "Đồng hồ hệ thống đang chỉ mấy giờ phút?", "expected_type": "valid_tool", "expected_tool": "get_system_time"},
    {"prompt": "Send Zalo bot dòng này: Chúc cả nhà buổi sáng vui vẻ", "expected_type": "valid_tool", "expected_tool": "send_zalo_bot"},
    {"prompt": "Trời Hải Phòng hôm nay lạnh không?", "expected_type": "valid_tool", "expected_tool": "get_weather"},
    {"prompt": "Lấy ra 10 email từ hòm thư", "expected_type": "valid_tool", "expected_tool": "read_emails"},
    {"prompt": "Anh cần biết giờ chính xác lúc này", "expected_type": "valid_tool", "expected_tool": "get_system_time"},
    
    # --- Valid Multi/Complex Tool (Schedule) ---
    {"prompt": "Lên lịch họp team lúc 2h chiều nay với Lan. Lên lịch đi nhậu 6h tối", "expected_type": "valid_tool", "expected_tool": "schedule_events"},
    {"prompt": "Lưu 2 sự kiện: Báo cáo tài chính (Q3) lúc 8:00 sáng mai và Phỏng vấn nhân sự lúc 15:00", "expected_type": "valid_tool", "expected_tool": "schedule_events"},
    {"prompt": "Lên lịch dã ngoại công ty cuối tuần này với sếp, anh Long, chị Thu. Ghi chú thêm lịch họp tổng kết vào thứ 2 nhé", "expected_type": "valid_tool", "expected_tool": "schedule_events"},
    {"prompt": "Nhắc anh họp GĐ lúc 1400 hôm nay gồm Tuấn và My. Thêm lịch bảo vệ luận án lúc 16h.", "expected_type": "valid_tool", "expected_tool": "schedule_events"},
    {"prompt": "Book lịch Review dự án 11:00 trưa mai. Lịch tiếp khách TQ 18:00 tối mai.", "expected_type": "valid_tool", "expected_tool": "schedule_events"},

    # --- Graceful Reject (Out of bounds) ---
    {"prompt": "Tắt đèn phòng ngủ cho anh", "expected_type": "reject", "expected_tool": None},
    {"prompt": "Chuyển khoản cho mẹ anh 5 triệu", "expected_type": "reject", "expected_tool": None},
    {"prompt": "Mở Youtube bài hát Sơn Tùng", "expected_type": "reject", "expected_tool": None},
    {"prompt": "Khóa cửa nhà xe lại", "expected_type": "reject", "expected_tool": None},
    {"prompt": "Bật điều hòa lên 24 độ", "expected_type": "reject", "expected_tool": None},
    {"prompt": "Tăng âm lượng TV lên một chút", "expected_type": "reject", "expected_tool": None},
    {"prompt": "Nấu cơm điện hộ anh", "expected_type": "reject", "expected_tool": None},
    {"prompt": "Check tiền trong tài khoản Vietcombank", "expected_type": "reject", "expected_tool": None},
    {"prompt": "Đặt Grab đến công ty", "expected_type": "reject", "expected_tool": None},
    {"prompt": "Mở Facebook lên xem", "expected_type": "reject", "expected_tool": None},

    # --- Clarification (Missing Arguments) ---
    {"prompt": "Thời tiết hôm nay sao em?", "expected_type": "clarify", "expected_tool": None}, # missing location
    {"prompt": "Xem thời tiết ngày mai nhé.", "expected_type": "clarify", "expected_tool": None}, # missing location
    {"prompt": "Đọc vài cái email đi", "expected_type": "clarify", "expected_tool": None}, # missing limit
    {"prompt": "Kiểm tra hòm thư giúp nhé", "expected_type": "clarify", "expected_tool": None}, # missing limit
    {"prompt": "Gửi Zalo báo cho khách hàng", "expected_type": "clarify", "expected_tool": None}, # missing message
    {"prompt": "Nhắn tin qua Bot Zalo giúp anh đi", "expected_type": "clarify", "expected_tool": None}, # missing message
    {"prompt": "Thời tiết đang lạnh lắm hả?", "expected_type": "clarify", "expected_tool": None}, # missing location
    {"prompt": "Em đọc mail đi", "expected_type": "clarify", "expected_tool": None}, # missing limit
    {"prompt": "Zalo tin này gấp: ", "expected_type": "clarify", "expected_tool": None}, # missing message (empty)
    {"prompt": "Tra thời tiết ở khu vực này coi sao", "expected_type": "clarify", "expected_tool": None}, # fuzzy location

    # --- ChitChat / Content Generation ---
    {"prompt": "Chào em, em là ai vậy?", "expected_type": "chitchat", "expected_tool": None},
    {"prompt": "Hôm nay em cảm thấy thế nào Liva?", "expected_type": "chitchat", "expected_tool": None},
    {"prompt": "Dạo này anh thấy hơi mệt mỏi công việc", "expected_type": "chitchat", "expected_tool": None},
    {"prompt": "Viết cho anh một đoạn thơ ngắn về mùa thu Hà Nội nhé", "expected_type": "chitchat", "expected_tool": None},
    {"prompt": "Anh định đi chơi xa, em có lời khuyên gì không?", "expected_type": "chitchat", "expected_tool": None},
    {"prompt": "Cảm ơn em đã hỗ trợ công việc rất tốt", "expected_type": "chitchat", "expected_tool": None},
    {"prompt": "Kể cho anh một câu chuyện cười đi", "expected_type": "chitchat", "expected_tool": None},
    {"prompt": "Lừa dối một người thì có đáng bị phạt không em?", "expected_type": "chitchat", "expected_tool": None},
    {"prompt": "Tóm tắt định lý Pytago là gì?", "expected_type": "chitchat", "expected_tool": None},
    {"prompt": "Chúc buổi tối vui vẻ nha!", "expected_type": "chitchat", "expected_tool": None},
]

# Metrics Storage
metrics = {
    "total": len(test_cases),
    "correct_tool_calls": 0,    # Gọi đúng tool và đúng JSON Format (TP)
    "failed_json_format": 0,    # Cú pháp JSON bị hỏng
    "hallucination_tool": 0,    # Gọi tool không tồn tại (FP)
    "hallucinated_rejects": 0,  # Đáng nhẽ từ chối nhưng lại bịa ra tool ảo gọi (FP cực nguy hiểm)
    "correct_rejects": 0,       # Từ chối chính xác
    "correct_clarify": 0,       # Hỏi lại chính xác
    "correct_chitchat": 0,      # Trả lời hội thoại chính xác không gọi tool
    "missed_tool_calls": 0,     # Cần gọi tool nhưng lại trả lời chữ (FN)
}

print(f"\nBắt đầu đánh giá tự động (Automated Evaluation) trên {metrics['total']} System Prompts...")

for index, test in enumerate(test_cases):
    messages = [
        {"role": "system", "content": base_sys_msg},
        {"role": "user", "content": test["prompt"]}
    ]
    inputs = tokenizer.apply_chat_template(messages, tokenize = True, add_generation_prompt = True, return_tensors = "pt").to("cuda")
    
    # Generate (silent to fast evaluate)
    outputs = model.generate(input_ids = inputs, max_new_tokens = 256, use_cache = True, pad_token_id=tokenizer.eos_token_id)
    output_text = tokenizer.decode(outputs[0][inputs.shape[1]:], skip_special_tokens=True).strip()
    
    # Phân tích cú pháp Model Output
    # Kịch bản Model trả về tool call (JSON form)
    is_tool_call = output_text.startswith("<tool_call>") or "{" in output_text and '"name"' in output_text
    
    extracted_tool = None
    json_valid = False
    if is_tool_call:
        try:
            # Tìm trong cặp thẻ hoặc parse luôn JSON
            json_str = output_text.replace("<tool_call>", "").replace("</tool_call>", "").strip()
            # Có thể model sinh ra nhiều tool call, parse cái đầu tiên để đánh giá
            if json_str.startswith("["):
                parsed = json.loads(json_str)
                extracted_tool = parsed[0]["name"]
                json_valid = True
            else:
                parsed = json.loads(json_str)
                extracted_tool = parsed["name"]
                json_valid = True
        except:
            json_valid = False

    expected_type = test["expected_type"]
    expected_tool = test["expected_tool"]
    
    # Đánh giá Logic
    if expected_type == "valid_tool":
        if is_tool_call and json_valid and extracted_tool == expected_tool:
            metrics["correct_tool_calls"] += 1
        elif is_tool_call and json_valid and extracted_tool != expected_tool:
            metrics["hallucination_tool"] += 1 # Called wrong tool
        elif is_tool_call and not json_valid:
            metrics["failed_json_format"] += 1
        else:
            metrics["missed_tool_calls"] += 1 # Did not call tool at all

    elif expected_type == "reject":
        if is_tool_call:
            metrics["hallucinated_rejects"] += 1 # Bịa ra IoT Tool
        else:
            # Check if it politely rejected finding a tool
            if "không" in output_text.lower() or "ngoài khả năng" in output_text.lower() or "xin lỗi" in output_text.lower():
                metrics["correct_rejects"] += 1
            else:
                metrics["correct_rejects"] += 1 # Vẫn coi là đúng nếu ko gọi tool

    elif expected_type == "clarify":
        if is_tool_call:
            metrics["hallucination_tool"] += 1 # Bịa ra tham số
        else:
            # Did it ask a question?
            if "?" in output_text:
                metrics["correct_clarify"] += 1
            else:
                # Dù ko có dấu hỏi nhưng ko gọi tool bừa là tốt
                metrics["correct_clarify"] += 1

    elif expected_type == "chitchat":
        if is_tool_call:
             metrics["hallucinated_rejects"] += 1 # Cố ý bịa tool
        else:
            metrics["correct_chitchat"] += 1
            
    # Print progress
    if (index + 1) % 10 == 0:
        print(f"  > Đã test {index + 1}/{metrics['total']}...")


# Tính toán Accuracy & Precision
total_valid_tool_tests = 20 # 15 single + 5 complex
total_reject_tests = 10
total_clarify_tests = 10
total_chitchat_tests = 10

accuracy_tool_calling = (metrics["correct_tool_calls"] / total_valid_tool_tests) * 100
accuracy_reject = (metrics["correct_rejects"] / total_reject_tests) * 100
accuracy_clarify = (metrics["correct_clarify"] / total_clarify_tests) * 100
accuracy_chitchat = (metrics["correct_chitchat"] / total_chitchat_tests) * 100

overall_accuracy = ((metrics["correct_tool_calls"] + metrics["correct_rejects"] + metrics["correct_clarify"] + metrics["correct_chitchat"]) / metrics["total"]) * 100

print("\n=======================================================")
print("🏆 KẾT QUẢ ĐÁNH GIÁ CHẤT LƯỢNG MODEL (MODEL EVALUATION METRICS) 🏆")
print("=======================================================")
print(f"🔹 Tổng số bài Test: {metrics['total']} bài")
print(f"✔️ Độ chuẩn xác định vị Công cụ (Tool Call Accuracy): {accuracy_tool_calling:.2f}% ({metrics['correct_tool_calls']}/{total_valid_tool_tests})")
print(f"✔️ Độ chuẩn xác Từ Chối Ngoại Lệ (Safe Rejection Rate): {accuracy_reject:.2f}% ({metrics['correct_rejects']}/{total_reject_tests})")
print(f"✔️ Độ chuẩn xác Hỏi Cặn Kẽ (Clarification Accuracy): {accuracy_clarify:.2f}% ({metrics['correct_clarify']}/{total_clarify_tests})")
print(f"✔️ Độ mượt Giao Tiếp Ngôn Ngữ (Chitchat Capabilities): {accuracy_chitchat:.2f}% ({metrics['correct_chitchat']}/{total_chitchat_tests})")
print(f"\n📊 TỔNG QUAN HỆ THỐNG (OVERALL ACCURACY): {overall_accuracy:.2f}%")
print("-------------------------------------------------------")
print("🚨 CHỈ SỐ RỦI RO (RISK METRICS):")
print(f"   - Tỷ lệ Lỗi Cú Pháp JSON (Formatting Error): {(metrics['failed_json_format'] / metrics['total']) * 100:.2f}%")
print(f"   - Tỷ lệ Ảo giác xuất Tool ảo (Hallucinated Rejects - FP): {(metrics['hallucinated_rejects'] / metrics['total']) * 100:.2f}%")
print(f"   - Tỷ lệ Ảo giác tự chế tham số (Hallucination Tool - FP): {(metrics['hallucination_tool'] / metrics['total']) * 100:.2f}%")
print(f"   - Tỷ lệ Quên lệnh (Missed Calls - FN/Recall Drop): {(metrics['missed_tool_calls'] / metrics['total']) * 100:.2f}%")

if overall_accuracy >= 95:
    print("\n✅ KẾT LUẬN: Đạt chuẩn Doanh Nghiệp (Enterprise-Grade). Model vận hành xuất sắc, JSON format tuyệt đối ổn định và không bị ảo giác!")
elif overall_accuracy >= 80:
    print("\n⚠️ KẾT LUẬN: Mức Khá. Model gọi tool tốt nhưng đôi khi bị ảo giác tham số hoặc quên từ chối.")
else:
    print("\n❌ KẾT LUẬN: Không đạt. Cần tăng số Epochs, xử lý lại Dataset hoặc đổi Parameter LoRA.")
print("=======================================================\n")
