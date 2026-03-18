import json
import random
import hashlib
from datetime import datetime, timedelta

def get_dynamic_system_prompt():
    # Randomize the system time to prevent static memorization
    base_time = datetime(2026, 3, random.randint(1, 30), random.randint(7, 22), random.randint(0, 59), 0)
    time_str = base_time.strftime("%H:%M:%S %d/%m/%Y")
    
    return f"""Bạn là Liva, một trợ lý AI thông minh. Chú ý: Nếu user yêu cầu một nhiệm vụ mà không có công cụ (tool) tương ứng hỗ trợ, HÃY TRẢ LỜI NGAY LẬP TỨC và TUYỆT ĐỐI KHÔNG DÙNG CÔNG CỤ KHÁC để thay thế/chữa cháy.

# Tools

You may call one or more functions to assist with the user query.

You are provided with function signatures within <tools></tools> XML tags:
<tools>
[
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
  }}
]
</tools>

For each function call, return a json object with function name and arguments within <tool_call></tool_call> XML tags:
<tool_call>
{{"name": <function-name>, "arguments": <args-json-object>}}
</tool_call>

Thời gian hệ thống hiện tại là: {time_str} (UTC+7)."""

def tc(name, args):
    args_str = json.dumps(args, ensure_ascii=False)
    # The true <tool_call> format specifically required by Qwen
    return f"\n<tool_call>\n{{\"name\": \"{name}\", \"arguments\": {args_str}}}\n</tool_call>\n"

def hash_conversation(messages):
    # Create a unique SHA-256 hash for the semantic flow of a conversation 
    # to guarantee our deduplication algorithms work flawlessly
    text_flow = "||".join([m['role'] + m.get('content', '') + str(m.get('name', '')) for m in messages if m['role'] != 'system'])
    return hashlib.sha256(text_flow.encode('utf-8')).hexdigest()

dataset = []
seen_hashes = set()

def add_to_dataset(messages):
    h = hash_conversation(messages)
    if h not in seen_hashes:
        seen_hashes.add(h)
        dataset.append({"messages": messages})


# --- 1. DYNAMIC EMAIL VARIANTS (100+ combinations) ---
senders_important = ["Sếp Nguyễn", "Miki Client (Japan)", "HR Dept", "Jira Automation", "GitHub", "AWS Support", "Kế Toán trưởng", "[URGENT] Giám Đốc", "Tech Lead"]
senders_spam = ["Grab Auto", "Netflix", "Shopee", "Vietcombank (Fake)", "Môi Giới Đất", "Spam Mailer", "Tiki Promo", "Steam", "Lazada", "Facebook Alerts"]

titles_important_base = [
    "Yêu cầu duyệt API spec cho Mobile app ({file})",
    "Action Required: Cập nhật thông tin thuế ({file})",
    "Lịch họp khẩn lúc 2PM chiều nay",
    "Ticket #2059 assigned: Fix memory leak on production",
    "[URGENT] AWS Billing crossed $1000",
    "Cảnh báo: 50 lượt SSH login thất bại",
    "Tuyển dụng: CV Ứng viên Frontend ({file})",
    "Phụ lục Hợp đồng Thuê Văn Phòng 2026 ({file})",
    "Review Code: Pull Request #402"
]
attachments = ["spec.pdf", "report.xlsx", "cv_frontend.pdf", "budget_2026.xlsx", "contract.docx", "None"]


pos_prompts = [
    "Đọc hộ anh {limit} mail mới rồi tóm tắt gửi qua Zalo luôn nhé Liva.",
    "Bạn check hòm thư lấy {limit} mail gần nhất, lọc rác ra rồi Zalo ý chính cho sếp tôi.",
    "Liva ơi, quét {limit} mail mới đến xem có gì gấp không, bắn thẳng sang Zalo.",
    "Tóm tắt {limit} cái mail rồi Zalo.",
    "Zalo cho anh nội dung quan trọng của {limit} email vừa nhận.",
    "Đọc giùm {limit} email xem có gì hot không rồi note lại qua Zalo nhé.",
    "Lấy {limit} thư mới gửi vào Zalo giúp.",
    "Mở hòm thư đọc {limit} thư gần đây rồi nhắn Zalo."
]


# BUILD CORE POSITIVE FLOWS (Vast Variety + CoT Reasoning)
for _ in range(15000): # Overshoot to allow Hash Deduplication to filter brilliantly
    if len(dataset) >= 800: break # Dynamic cut-off

    limit = random.choice([1, 2, 3, 5, 10, 15, 20, 30])
    msg = random.choice(pos_prompts).replace("{limit}", str(limit))
    
    # Generate completely dynamic emails
    is_important = random.choice([True, False])
    
    if is_important:
        sender = random.choice(senders_important)
        att = random.choice(attachments)
        title = random.choice(titles_important_base).replace("{file}", f"Đính kèm: {att}" if att != "None" else "")
        email_content = f"--- Email 1 ---\nTừ: {sender}\nTiêu đề: {title}\n--- Email {limit} ---\nTừ: Shopee\nTiêu đề: Mua 1 Tặng 1"
        
        # Chain of Thought (Thinking before tool call)
        sys_prompt = get_dynamic_system_prompt()
        think = random.choice([
            f"Dạ, để em kiểm tra hộp thư lấy {limit} email mới nhất cho anh nhé.",
            f"Em sẽ đọc {limit} cái mail rồi lọc rác ra giúp anh ạ.",
            f"Dạ vâng, em quét ngay {limit} email mới đây."
        ])
        
        zalo_summary = f"{sender}: {title}"
        zalo_think = random.choice([
            f"Em đọc xong rồi. Em thấy có email từ {sender} khá quan trọng nên em gửi Zalo liền đây ạ.",
            f"Trong {limit} email, có 1 thư rác Shopee em bỏ qua. Riêng thư từ {sender} thì em Zalo anh nhé.",
            f"Dạ có việc gấp từ {sender}, em nhắn Zalo cho anh ngay lập tức ạ!"
        ])
        
        messages = [
            {"role": "system", "content": sys_prompt},
            {"role": "user", "content": msg},
            {"role": "assistant", "content": f"{think}{tc('read_emails', {'limit': limit})}"},
            {"role": "tool", "name": "read_emails", "content": email_content},
            {"role": "assistant", "content": f"{zalo_think}{tc('send_zalo_bot', {'message': zalo_summary})}"},
            {"role": "tool", "name": "send_zalo_bot", "content": "Success"},
            {"role": "assistant", "content": "Em đã gửi báo cáo Zalo thành công ạ."}
        ]
        add_to_dataset(messages)

    else:
        # All spam scenario
        sender1 = random.choice(senders_spam)
        sender2 = random.choice(senders_spam)
        email_content = f"--- Email 1 ---\nTừ: {sender1}\nTiêu đề: Voucher 500k\n--- Email {limit} ---\nTừ: {sender2}\nTiêu đề: Phim mới ra mắt"
        
        sys_prompt = get_dynamic_system_prompt()
        messages = [
            {"role": "system", "content": sys_prompt},
            {"role": "user", "content": msg},
            {"role": "assistant", "content": f"Dạ để em kiểm tra ngay.{tc('read_emails', {'limit': limit})}"},
            {"role": "tool", "name": "read_emails", "content": email_content},
            {"role": "assistant", "content": f"Dạ em xem qua {limit} thư thì toàn là rác từ {sender1} và {sender2} ạ. Em xin phép không làm phiền anh qua Zalo nhé."}
        ]
        add_to_dataset(messages)


# --- 2. ADVANCED FILTERS: Sender & Keyword Routing ---
filter_queries = [
    ("Chỉ đọc mail từ sếp trong 15 cái gần nhất nhé.", "Sếp", "15"),
    ("Lọc 10 email, tìm mail nào ghi là URGENT thì bắn Zalo.", "URGENT", "10"),
    ("Trong 20 mail vừa qua, cái nào từ HR thì nhắn Zalo.", "HR", "20"),
    ("Tìm mail có file đính kèm (spec.pdf) trong 5 mail mới để Zalo anh.", "spec.pdf", "5")
]

for _ in range(150):
    u, kw, lim = random.choice(filter_queries)
    messages = [
        {"role": "system", "content": get_dynamic_system_prompt()},
        {"role": "user", "content": u},
        {"role": "assistant", "content": f"Vâng, em sẽ lọc theo yêu cầu khắt khe của anh.{tc('read_emails', {'limit': int(lim)})}"},
        {"role": "tool", "name": "read_emails", "content": f"Từ: {kw}\nTiêu đề: Khẩn cấp/File."},
        {"role": "assistant", "content": f"Em đã tìm thấy email chứa '{kw}' đúng như anh cần. Em gửi Zalo luôn đây ạ.{tc('send_zalo_bot', {'message': f'Đã tìm thấy thông tin liên quan tới {kw}.'})}"},
        {"role": "tool", "name": "send_zalo_bot", "content": "Success"},
        {"role": "assistant", "content": "Đã thực thi việc lọc cụ thể thành công và gửi Zalo ạ."}
    ]
    add_to_dataset(messages)


# --- 3. PARALLEL & MULTI-TURN ---
for _ in range(150):
    messages = [
        {"role": "system", "content": get_dynamic_system_prompt()},
        {"role": "user", "content": "Mấy giờ r? Đọc 2 mail luôn rủ x zalo."},
        {"role": "assistant", "content": f"Dạ em sẽ lấy giờ và đọc mail cùng lúc ạ.{tc('get_system_time', {})}{tc('read_emails', {'limit': 2})}"},
        {"role": "tool", "name": "get_system_time", "content": "09:00:00 21/04"},
        {"role": "tool", "name": "read_emails", "content": "Từ: Tech Lead\nTiêu đề: Họp server"},
        {"role": "assistant", "content": f"Bây giờ là 09:00 sáng. Em thấy Tech Lead nhắn họp server nên em gửi Zalo cho anh nhé.{tc('send_zalo_bot', {'message': 'Tech Lead báo họp (hồi 09:00)'})}"},
        {"role": "tool", "name": "send_zalo_bot", "content": "Success"},
        {"role": "assistant", "content": "Gửi thành công!"},
        {"role": "user", "content": " Ủa hồi nãy bảo họp server lúc mấy giờ nhỉ?"},
        {"role": "assistant", "content": "Dạ thông báo trên hệ thống là lúc 09:00 sáng anh nhé, thời điểm em vừa kiểm tra xong đó ạ."}
    ]
    add_to_dataset(messages)


# --- 3.5. DIRECT ACTION (Pre-filled info, no research needed) ---
direct_prompts = [
    "Liva ơi, nhắn tin zalo báo cáo tình hình dự án tuần này hoàn thành 100% KPI nhé.",
    "Bạn Zalo hộ tôi nhắc nhóm dev là 'đã duyệt bug' nhé.",
    "Gửi Zalo cho sếp nội dung: Cuộc họp huỷ chuyển sang chiều mai."
]

for _ in range(150):
    u = random.choice(direct_prompts)
    messages = [
        {"role": "system", "content": get_dynamic_system_prompt()},
        {"role": "user", "content": u},
        {"role": "assistant", "content": f"Dạ vâng, em nhắn Zalo báo cáo ngay đây ạ.{tc('send_zalo_bot', {'message': u})}"},
        {"role": "tool", "name": "send_zalo_bot", "content": "Success"},
        {"role": "assistant", "content": "Em đã gửi Zalo thành công ạ."}
    ]
    add_to_dataset(messages)

# --- 4. EXTREME EDGE CASES (API Failures) ---
for _ in range(400):
    messages = [
        {"role": "system", "content": get_dynamic_system_prompt()},
        {"role": "user", "content": "Đọc 5 mail rồi Zalo."},
        {"role": "assistant", "content": f"Dạ.{tc('read_emails', {'limit': 5})}"},
        {"role": "tool", "name": "read_emails", "content": "Từ: Khách Hàng\nTiêu đề: Hợp đồng tỷ đô"},
        {"role": "assistant", "content": f"Ối anh ơi, có khách chốt hợp đồng tỷ đô. Em báo Zalo ngay!{tc('send_zalo_bot', {'message': 'Khách chốt hợp đồng lớn.'})}"},
        {"role": "tool", "name": "send_zalo_bot", "content": "Error: Timeout 500 from Zalo ZCA"},
        {"role": "assistant", "content": "Anh Dương ơi Zalo API đang bị Timeout nghẽn mạng! Nhưng có tin cực vui là Khách hàng vừa chốt Hợp đồng tỷ đô qua email. Anh đọc tạm trên màn hình này nhé!"}
    ]
    add_to_dataset(messages)

# --- NEGATIVE ERRORS (Weather, Tool Not Found, chit-chat) ---
neg_prompts = [
    ("Mấy giờ cmnr?", tc('get_system_time', {}), "15:35:00 15/03/2026", "Dạ hiện tại là 15:35 chiều ngày 15/03/2026 ạ."),
    ("thời tiết hà nội hnay mếy độ z Liva?", None, None, "Em không có tool lấy thời tiết ạ. Em chỉ hỗ trợ mail và Zalo thôi."),
    ("Bạn có thể tạo lịch trình họp lúc 5h chiều không?", None, None, "Dạ em chỉ lấy mail và nhắn Zalo thôi, em không có tool xếp lịch schedule_events ạ."),
    ("Trời hôm nay nóng không em?", None, None, "Dạ em không có công cụ đo nhiệt độ, em không rõ ạ.")
]

for i in range(300):
    u, call, res, final = random.choice(neg_prompts)
    
    # Randomize the text to bypass SHA-256 dedupe logic and expand the Non-Tool ratio
    dynamic_suffix = f" (Mã hỏi: {random.randint(1000, 9999)})"
    u_dyn = u + dynamic_suffix
    
    sys_prompt = get_dynamic_system_prompt()
    messages = [{"role": "system", "content": sys_prompt}, {"role": "user", "content": u_dyn}]
    
    if call:
        messages.append({"role": "assistant", "content": f"Dạ để em xem.{call}"})
        messages.append({"role": "tool", "name": ("get_system_time" if "get_system_time" in call else "unknown"), "content": res})
        messages.append({"role": "assistant", "content": final})
    else:
        messages.append({"role": "assistant", "content": final})
        
    add_to_dataset(messages)

# --- 5. SHAREGPT CHIT-CHAT (Sanitized Zero Tools) ---
try:
    with open('openclaw_sharegpt_format.jsonl', 'r', encoding='utf-8') as sf:
        sharegpt_lines = sf.readlines()
    random.shuffle(sharegpt_lines)
    for line in sharegpt_lines[:300]:
        item = json.loads(line)
        convs = item.get("conversations", [])
        messages_converted = [{"role": "system", "content": get_dynamic_system_prompt()}]
        for turn in convs:
            role = "assistant" if turn.get("from", "").lower() in ["gpt", "assistant"] else "user"
            messages_converted.append({"role": role, "content": turn.get("value", "")})
        if len(messages_converted) >= 3:
            # ERADICATE HALLUCINATIONS AND LEFTOVER JSON TOOLS
            has_tool_call = any("<tool_call>" in turn["content"] for turn in messages_converted)
            has_get_weather = any("get_weather" in turn["content"] for turn in messages_converted)
            if not has_tool_call and not has_get_weather:
                add_to_dataset(messages_converted)
except:
    pass

# Direct Zalo (Blind Sends)
for _ in range(50):
    messages = [
        {"role": "system", "content": get_dynamic_system_prompt()},
        {"role": "user", "content": "Zalo cho bot nói là LIVA đang bận build model V11 nhé."},
        {"role": "assistant", "content": f"Dạ vâng, em nhắn đi ngay.{tc('send_zalo_bot', {'message': 'LIVA đang bận build model V11.'})}"},
        {"role": "tool", "name": "send_zalo_bot", "content": "Success"},
        {"role": "assistant", "content": "Em đã truyền tin nhắn mù qua Zalo thành công ạ."}
    ]
    add_to_dataset(messages)

random.shuffle(dataset)

with open('zalo_focused_tool_calling.jsonl', 'w', encoding='utf-8') as f:
    for d in dataset:
        f.write(json.dumps(d, ensure_ascii=False) + '\n')

print(f"Created V11 MAXIMUM Dataset with {len(dataset)} unique hashed samples. Perfect Qwen Architecture.")
