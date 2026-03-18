import json
import random

SYSTEM_PROMPT = """Bạn là Liva, một trợ lý AI thông minh.

# Tools

You may call one or more functions to assist with the user query.

You are provided with function signatures within <tools></tools> XML tags:
<tools>
[
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
  }
]
</tools>

For each function call, return a json object with function name and arguments within <tool_call></tool_call> XML tags:
<tool_call>
{"name": <function-name>, "arguments": <args-json-object>}
</tool_call>

Thời gian hệ thống hiện tại là: 15:30:00 15/03/2026 (UTC+7)."""

def tc(name, args):
    args_str = json.dumps(args, ensure_ascii=False)
    # Cú pháp chuẩn xác nhất theo Qwen XML Tool Calling Prompt
    return f"\n<tool_call>\n{{\"name\": \"{name}\", \"arguments\": {args_str}}}\n</tool_call>\n"

dataset = []

pos_prompts = [
    "Giúp tôi tóm tắt {limit} gmail gần nhất rồi gửi qua zalo nhé.",
    "Bạn đọc {limit} email mới nhất, tóm gọn lại rồi nhắn qua Zalo bot cho tôi.",
    "Check hộp thư lấy {limit} mail mới, tóm tắt ý chính rồi chuyển tiếp nội dung qua Zalo nha Liva.",
    "Có email nào mới không, lấy {limit} cái tóm tắt rồi gửi Zalo đi.",
    "Lọc {limit} cái mail rồi Zalo cho tôi.",
    "Zalo cho anh nội dung của {limit} email vừa đến.",
    "Đọc giùm anh {limit} email đầu tiên nhé Liva, có gì quan trọng Zalo anh.",
    "Tóm tắt {limit} email mới nhất và gửi Zalo.",
    "Lấy {limit} thư mới gửi vào Zalo giúp.",
    "Mở hòm thư đọc {limit} mail gần đây rồi note lại qua Zalo."
]

email_templates = [
    ("--- Email 1 ---\nTừ: Sếp Nguyễn\nTiêu đề: [Khẩn cấp] Báo cáo doanh thu quý 3\n--- Email {limit} ---\nTừ: Lazada\nTiêu đề: Flash Sale 50%", 
     "Sếp Nguyễn yêu cầu khẩn Báo cáo doanh thu quý 3.", 
     "Dạ em đã lọc {limit} email. Có mail khẩn từ Sếp Nguyễn nên em báo Zalo cho anh trước nhé."),
     
    ("--- Email 1 ---\nTừ: Khách hàng Nhật\nTiêu đề: Change request for login page\nĐính kèm: spec.pdf\n--- Email {limit} ---\nTừ: Facebook\nTiêu đề: Ai đó đã nhắc đến bạn", 
     "Khách Nhật gửi file spec.pdf yêu cầu sửa trang Login.", 
     "Dạ em đọc {limit} thư xong rồi. Khách Nhật vừa gửi yêu cầu đổi màn hình Login kèm theo file đính kèm. Thư Facebook rác em bỏ qua ạ. Em Zalo anh nhé!"),
     
    ("--- Email 1 ---\nTừ: AWS Alerts\nTiêu đề: Billing alarm - $500 threshold exceeded\n--- Email {limit} ---\nTừ: Nguyen B\nTiêu đề: Xin file excel hôm qua", 
     "1. AWS: Báo động cước phí vượt quá $500.\\n2. Nguyen B: Xin file excel.", 
     "Trong {limit} email, em thấy có cảnh báo vượt cước 500$ từ AWS và anh Nguyen B xin file. Em đã tóm tắt và gửi Zalo cho anh ngay lập tức ạ."),
     
    ("--- Email 1 ---\nTừ: HR Bộ phận IT\nTiêu đề: Thông báo lịch khám sức khỏe\n--- Email {limit} ---\nTừ: Jira\nTiêu đề: Ticket #1024 has been assigned to you", 
     "1. HR thông báo lịch khám sức khỏe.\\n2. Jira báo có ticket #1024 được gán.", 
     "Dạ, em check xong {limit} email rồi ạ. Có lịch khám sức khỏe với 1 ticket mới trên Jira, em đã push nội dung qua Zalo cho anh Dương!"),
     
    ("--- Email 1 ---\nTừ: Security Node\nTiêu đề: SSH login failed 50 times\n--- Email {limit} ---\nTừ: Vietcombank\nTiêu đề: Cảnh báo truy cập", 
     "🔥 Chú ý: Có 50 lần SSH login thất bại và VCB cảnh báo truy cập lạ.", 
     "Anh ơi khẩn cấp, {limit} email này toàn cảnh báo bảo mật. Có nguy cơ bị brute-force SSH và VCB báo truy cập lạ. Em gửi thẳng Zalo anh xử lý nha!"),
     
    ("--- Email 1 ---\nTừ: Vợ\nTiêu đề: Tối nay ăn gì anh?\n--- Email {limit} ---\nTừ: Sentry Dashboard\nTiêu đề: 502 Bad Gateway - liva-ai-engine", 
     "Vợ hỏi tối nay ăn gì. Máy chủ liva-ai-engine đang bị 502 Bad Gateway.", 
     "Có tin từ vợ và cả lỗi 502 trên server Liva nữa. Em gửi Zalo 2 mục này luôn cho {limit} mail này nhé."),
     
    ("--- Email 1 ---\nTừ: Shopee\nTiêu đề: Đơn hàng đã giao thành công\n--- Email {limit} ---\nTừ: Môi giới đất\nTiêu đề: Cần bán mảnh đất view đẹp", 
     "Không có thông tin quan trọng.", 
     "Dạ em xem qua {limit} thư này thì toàn là Shopee với môi giới đất. Rất tiếc không có thư nào quan trọng để tóm tắt Zalo ạ.")
]

# 1. POSITIVE (Read + Send Zalo) - MỤC TIÊU: 1500 (Giờ giảm còn ~500 chuẩn)
for _ in range(500):
    limit = random.choice([1, 2, 3, 5, 7, 10, 15, 20])
    msg = random.choice(pos_prompts).replace("{limit}", str(limit))
    
    # 7 Templates, pick 1 randomly
    tpl = random.choice(email_templates)
    email_content = tpl[0].replace("{limit}", str(limit))
    zalo_msg = tpl[1]
    reasoning = tpl[2].replace("{limit}", str(limit))
    
    if zalo_msg == "Không có thông tin quan trọng.":
        messages = [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": msg},
            {"role": "assistant", "content": f"Để em kiểm tra {limit} email mới nhất cho anh nhé.{tc('read_emails', {'limit': limit})}"},
            {"role": "tool", "name": "read_emails", "content": email_content},
            {"role": "assistant", "content": reasoning}
        ]
    else:
        messages = [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": msg},
            {"role": "assistant", "content": f"Dạ để em kiểm tra hộp thư lấy {limit} email mới nhé.{tc('read_emails', {'limit': limit})}"},
            {"role": "tool", "name": "read_emails", "content": email_content},
            {"role": "assistant", "content": f"{reasoning}{tc('send_zalo_bot', {'message': zalo_msg})}"},
            {"role": "tool", "name": "send_zalo_bot", "content": "Success"},
            {"role": "assistant", "content": "Em gừi xong qua Zalo Bot rồi anh nhé."}
        ]
    dataset.append({"messages": messages})

# 2. DIRECT ZALO SENDS (Direct without emails)
for _ in range(100):
    msgs = [("Nhắn Zalo cho sếp là tôi đến muộn 15p.", "Tôi đến muộn 15p."), ("Push zalo báo server đang bảo trì giúp.", "Server đang bảo trì.")]
    u, bot = random.choice(msgs)
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": u},
        {"role": "assistant", "content": f"Dạ em sẽ gửi thông báo này đi ngay ạ.{tc('send_zalo_bot', {'message': bot})}"},
        {"role": "tool", "name": "send_zalo_bot", "content": "Success"},
        {"role": "assistant", "content": "Em đã nhắn Zalo xong rồi ạ."}
    ]
    dataset.append({"messages": messages})

# 3. ZALO API ERROR CASE
for _ in range(100):
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": "Đọc 3 mail rồi gửi Zalo đi em."},
        {"role": "assistant", "content": f"Dạ, em đọc mail ngay đây.{tc('read_emails', {'limit': 3})}"},
        {"role": "tool", "name": "read_emails", "content": "--- Email 1 ---\nTừ: AWS\nTiêu đề: Bill"},
        {"role": "assistant", "content": f"Em thấy cái bill AWS, em gửi Zalo nhé.{tc('send_zalo_bot', {'message': 'AWS Bill'})}"},
        {"role": "tool", "name": "send_zalo_bot", "content": "Error 429: Rate limit exceeded"},
        {"role": "assistant", "content": "Anh ơi Zalo báo lỗi Rate limit (giới hạn tần suất) gửi quá nhanh nên em không thể push qua Zalo lúc này được. Anh check tạm trên màn hình này là có hóa đơn AWS nhé."}
    ]
    dataset.append({"messages": messages})

# 4. NEGATIVE ERRORS (Weather, Tool Not Found, chit-chat)
neg_prompts = [
    ("Mấy giờ cmnr?", tc('get_system_time', {}), "15:35:00 15/03/2026", "Dạ hiện tại là 15:35 chiều ngày 15/03/2026 ạ."),
    ("thời tiết hà nội hnay mếy độ z Liva?", None, None, "Em không có tool lấy thời tiết ạ."),
    ("Bạn có thể tạo lịch trình họp lúc 5h chiều không?", None, None, "Dạ em chỉ lấy mail và nhắn Zalo thôi, em không có tool xếp lịch schedule_events ạ."),
    ("Trời hôm nay nóng không em?", None, None, "Dạ em không có tool lấy thời tiết nên em không rõ ạ.")
]

for _ in range(200):
    u, call, res, final = random.choice(neg_prompts)
    messages = [{"role": "system", "content": SYSTEM_PROMPT}, {"role": "user", "content": u}]
    if call:
        messages.append({"role": "assistant", "content": f"Dạ để em xem.{call}"})
        messages.append({"role": "tool", "name": ("get_system_time" if "get_system_time" in call else "unknown"), "content": res})
        messages.append({"role": "assistant", "content": final})
    else:
        messages.append({"role": "assistant", "content": final})
    dataset.append({"messages": messages})

# 5. PARALLEL TOOL CALLS (Time + Emails)
for _ in range(100):
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": "Mấy giờ r? Đọc 2 mail luôn rủ x zalo."},
        {"role": "assistant", "content": f"Dạ vâng, em sẽ thực hiện đồng thời.{tc('get_system_time', {})}{tc('read_emails', {'limit': 2})}"},
        {"role": "tool", "name": "get_system_time", "content": "15:35:00"},
        {"role": "tool", "name": "read_emails", "content": "Email: Sếp nhắn họp"},
        {"role": "assistant", "content": f"Thời gian là 15:35 ạ. Email thì có Sếp nhắn họp, em gửi Zalo.{tc('send_zalo_bot', {'message': 'Sếp nhắn họp (15:35)'})}"},
        {"role": "tool", "name": "send_zalo_bot", "content": "Success"},
        {"role": "assistant", "content": "Gửi thành công!"}
    ]
    dataset.append({"messages": messages})

# 6. COMPLEX QUERIES
for _ in range(100):
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": "Chỉ đọc mail sếp trong 15 cái gần nhất nhé."},
        {"role": "assistant", "content": f"Vâng, em sẽ lọc thư từ Sếp trong 15 cái.{tc('read_emails', {'limit': 15})}"},
        {"role": "tool", "name": "read_emails", "content": "Từ: Sếp. Tiêu đề: Khẩn."},
        {"role": "assistant", "content": f"Đúng là có mail khẩn từ Sếp, em gửi anh qua Zalo.{tc('send_zalo_bot', {'message': 'Sếp báo khẩn.'})}"},
        {"role": "tool", "name": "send_zalo_bot", "content": "Success"},
        {"role": "assistant", "content": "Zalo hoàn tất."}
    ]
    dataset.append({"messages": messages})

# Merge ShareGPT (~500 mẫu)
try:
    with open('openclaw_sharegpt_format.jsonl', 'r', encoding='utf-8') as sf:
        sharegpt_lines = sf.readlines()
    random.shuffle(sharegpt_lines)
    for line in sharegpt_lines[:500]:
        item = json.loads(line)
        convs = item.get("conversations", [])
        messages_converted = [{"role": "system", "content": SYSTEM_PROMPT}]
        for turn in convs:
            role = "assistant" if turn.get("from", "").lower() in ["gpt", "assistant"] else "user"
            messages_converted.append({"role": role, "content": turn.get("value", "")})
        if len(messages_converted) >= 3:
            # Drop any ShareGPT lines containing tool calls to eradicate hallucinations
            has_tool_call = any("<tool_call>" in turn["content"] for turn in messages_converted)
            if not has_tool_call:
                dataset.append({"messages": messages_converted})
except:
    pass

random.shuffle(dataset)

with open('zalo_focused_tool_calling.jsonl', 'w', encoding='utf-8') as f:
    for d in dataset:
        f.write(json.dumps(d, ensure_ascii=False) + '\n')

print(f"Created V10 Dataset with {len(dataset)} samples. True XML Tool Calls + CoT implemented.")
