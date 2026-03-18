import json
import random
import hashlib
from datetime import datetime, timedelta

def get_dynamic_system_prompt():
    base_time = datetime(2026, 3, random.randint(1, 30), random.randint(7, 22), random.randint(0, 59), 0)
    time_str = base_time.strftime("%H:%M:%S %d/%m/%Y")
    
    return f"""Bạn là Liva, một trợ lý AI thông minh.

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
  }},
  {{
    "type": "function",
    "function": {{
      "name": "search_drive",
      "description": "Tìm kiếm file trên Google Drive (Lưu ý: Công cụ này hiện tại đang bị lỗi, không nên sử dụng).",
      "parameters": {{
        "type": "object",
        "properties": {{
          "query": {{"type": "string", "description": "Tên file cần tìm"}}
        }},
        "required": ["query"]
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
    return f"\n<tool_call>\n{{\"name\": \"{name}\", \"arguments\": {args_str}}}\n</tool_call>\n"

dataset = []
seen_hashes = set()

def get_hash(messages):
    text_flow = "||".join([m['role'] + m.get('content', '') + str(m.get('name', '')) for m in messages if m['role'] != 'system'])
    return hashlib.sha256(text_flow.encode('utf-8')).hexdigest()

def add_to_dataset(messages, difficulty="medium", tools_called=1):
    h = get_hash(messages)
    if h not in seen_hashes:
        seen_hashes.add(h)
        dataset.append({
            "metadata": {
                "difficulty": difficulty,
                "num_tools_called": tools_called,
                "hash": h[:8]
            },
            "messages": messages
        })


senders_important = ["Tech Lead", "GitHub PR", "Client Japan", "AWS Support", "HR Dept", "Jira Automation", "Giám Đốc", "Sếp Nguyễn", "IT Support"]
senders_spam = ["Grab Promo", "Netflix Update", "Fake Bank Alert", "Shopee", "Lazada", "Môi giới BĐS", "Tinder Verify"]
attachments = ["budget_2026.xlsx", "spec.pdf", "cv_frontend.pdf", "report.csv", "invoice_2026.pdf", "marketing_plan.pptx", "None"]
keywords_imp = ["URGENT", "Khẩn cấp", "AWS", "Jira", "GitHub PR", "Bug", "Report"]

zalo_rich_formats = [
    "🚨 Khẩn: @sếp {sender} báo {title}, cần check ngay ạ! 💻 (Đính kèm: {att})",
    "🔥 Anh Dương ơi có thư URGENT từ {sender}: {title} ⚠️",
    "📩 Báo cáo: {sender} vừa gửi thông tin quan trọng '{title}' nhé sếp. (Mở file {att})",
    "⚡ {sender} -> {title}. Em gửi sếp coi luôn nha!",
    "⚠️ Chú ý: {sender} yêu cầu xem {title}. Đã bỏ qua 1 đống mail rác cho sếp xong."
]

zero_email_phrases = [
    "Dạ em check rồi, toàn là thư rác với quảng cáo thôi anh ạ.",
    "Trong {limit} thư kiểm tra toàn là mail spam, không có cái nào quan trọng để tóm tắt Zalo ạ.",
    "Hòm thư của anh chả có gì gấp cả, toàn Grab với Shopee thôi sếp ơi.",
    "Dạ hòm thư trống trơn, chưa có mail mới nào anh ạ."
]


# 1. CORE FLOWS + 0-EMAILS + HIGH IMPORTANCE RATIO (Parallel Simulation)
for _ in range(20000):
    if len(dataset) >= 1500: break
    
    req_limit = random.randint(3, 50)
    act_count = random.choice([0, 1, 3, 5, req_limit])
    if act_count > req_limit: act_count = req_limit

    msg_prompts = [
        f"Đọc {req_limit} thư rồi zalo sếp.",
        f"Tóm tắt giúp anh {req_limit} mail gần nhất.",
        f"Quét {req_limit} email xem có gì khẩn không."
    ]
    u_msg = random.choice(msg_prompts)

    if act_count == 0:
        msgs = [
            {"role": "system", "content": get_dynamic_system_prompt()},
            {"role": "user", "content": u_msg},
            {"role": "assistant", "content": f"Dạ, em check mail ngay.{tc('read_emails', {'limit': req_limit})}"},
            {"role": "tool", "name": "read_emails", "content": "[]"},
            {"role": "assistant", "content": random.choice(["Dạ hòm thư anh hiện chưa có mail mới nào cả.", "Không có email nào về, inbx trống trơn sếp ạ."])}
        ]
        add_to_dataset(msgs, "easy", 1)
        continue

    tool_res = []
    
    # Simulate High Importance Ratio (20-30% chance for ALL IMPORTANT)
    is_all_important = random.random() < 0.25
    imp_items = []
    
    for i in range(1, act_count + 1):
        if is_all_important or (i == act_count): # Guarantee at least 1 important email (placed at the end to simulate actual reality)
            sd = random.choice(senders_important)
            at = random.choice(attachments)
            tl = random.choice(keywords_imp) + f": Yêu cầu xử lý ({i})"
            body = f"--- Email {i} ---\nTừ: {sd}\nTiêu đề: {tl}"
            if at != "None": body += f" (Đính kèm: {at})"
            tool_res.append(body)
            imp_items.append({"sender": sd, "title": tl, "att": at})
        else:
            tool_res.append(f"--- Email {i} ---\nTừ: {random.choice(senders_spam)}\nTiêu đề: Rác rưởi quảng cáo {i}")
            
    tool_content = "\n".join(tool_res)
    
    # Process important items into a rich Zalo string
    if len(imp_items) == 0: # Pure spam fallback
        msgs = [
            {"role": "system", "content": get_dynamic_system_prompt()},
            {"role": "user", "content": u_msg},
            {"role": "assistant", "content": f"Em xem ngay.{tc('read_emails', {'limit': req_limit})}"},
            {"role": "tool", "name": "read_emails", "content": tool_content},
            {"role": "assistant", "content": random.choice(zero_email_phrases).replace("{limit}", str(act_count))}
        ]
        add_to_dataset(msgs, "medium", 1)
    else:
        # Construct Zalo message
        if is_all_important:
            z_msg = f"🔥 Khẩn: Báo cáo sếp có {len(imp_items)} email siêu khẩn cấp đổ về cùng lúc từ các đối tác lớn. Em gửi sếp gộp nhé!"
            z_think = "Anh ơi, hòm thư đợt này toàn là mail VIP khẩn cấp không luôn ạ. Em gộp lại gửi Zalo cho sếp."
        else:
            first_imp = imp_items[0]
            z_msg = random.choice(zalo_rich_formats).replace("{sender}", first_imp["sender"]).replace("{title}", first_imp["title"]).replace("{att}", first_imp["att"])
            z_think = f"Mấy thư đầu toàn rác, may tới cái {act_count} là của {first_imp['sender']}. Em bắn Zalo luôn nghen."
            
        msgs = [
            {"role": "system", "content": get_dynamic_system_prompt()},
            {"role": "user", "content": u_msg},
            {"role": "assistant", "content": f"Dạ em xem cho.{tc('read_emails', {'limit': req_limit})}"},
            {"role": "tool", "name": "read_emails", "content": tool_content},
            {"role": "assistant", "content": f"{z_think}{tc('send_zalo_bot', {'message': z_msg})}"},
            {"role": "tool", "name": "send_zalo_bot", "content": "Success"},
            {"role": "assistant", "content": random.choice(["Dạ xong! Zalo thẳng tiến. Sếp bấm điện thoại check ngay nha 📱", "Em phi Zalo xong rồi ạ. Ngon ơ!", "Đã push Zalo thành công."])}
        ]
        add_to_dataset(msgs, "hard" if is_all_important else "medium", 2)


# 2. HARD NEGATIVES (Tool Errors, Wrong Params, Zalo Detail Restriction)
for _ in range(300):
    u = f"Gửi Zalo chi tiết ĐẦY ĐỦ hết 10 trang của cái mail nhé. {random.randint(1,999)}"
    msgs = [
        {"role": "system", "content": get_dynamic_system_prompt()},
        {"role": "user", "content": u},
        {"role": "assistant", "content": f"Dạ em check mail lấy thông tin trước.{tc('read_emails', {'limit': 1})}"},
        {"role": "tool", "name": "read_emails", "content": "--- Email 1 ---\nTừ: Đối tác\nTiêu đề: Hợp đồng 10 trang."},
        {"role": "assistant", "content": f"Anh ơi em không gửi nguyên văn 10 trang được đâu, Zalo báo lỗi rác đó. Em tóm tắt nội dung gọn lại gửi nha.{tc('send_zalo_bot', {'message': 'Đối tác gửi Hợp Đồng đính kèm.'})}"},
        {"role": "tool", "name": "send_zalo_bot", "content": "Success"},
        {"role": "assistant", "content": "Em gọt đi gửi tóm tắt ngon ơ rồi đó."}
    ]
    add_to_dataset(msgs, "hard", 2)

for _ in range(300):
    # Tool Failure -> Xin lỗi sửa sai
    msgs = [
        {"role": "system", "content": get_dynamic_system_prompt()},
        {"role": "user", "content": f"Đọc 5 thư cho anh. {random.randint(1,999)}"},
        {"role": "assistant", "content": f"Dạ vâng.{tc('read_emails', {'limit': 'năm'})}"}, # Cố tình sai kiểu
        {"role": "tool", "name": "read_emails", "content": "Error: 'limit' must be a number"},
        {"role": "assistant", "content": f"Ối em xin lỗi anh, em ghi nhầm tham số một xíu. Để em kiểm tra lại đàng hoàng.{tc('read_emails', {'limit': 5})}"},
        {"role": "tool", "name": "read_emails", "content": "--- Email 1 ---\nTừ: Mẹ\nTiêu đề: Về ăn cơm."},
        {"role": "assistant", "content": f"Có tin Mẹ gọi về ăn cơm, em nhắn Zalo anh nhé.{tc('send_zalo_bot', {'message': 'Mẹ hối về nhà ăn cơm nha sếp 🍚'})}"},
        {"role": "tool", "name": "send_zalo_bot", "content": "Success"},
        {"role": "assistant", "content": "Dạ em gửi xong xuôi rồi anh."}
    ]
    add_to_dataset(msgs, "hard", 3)


# 3. PARALLEL: Time + Mail + Zalo
for _ in range(300):
    msgs = [
        {"role": "system", "content": get_dynamic_system_prompt()},
        {"role": "user", "content": f"Thời điểm này là mấy giờ rồi? Check 3 email luôn xem. {random.randint(1,999)}"},
        {"role": "assistant", "content": f"Dạ em tra cứu đồng thời luôn đây.{tc('get_system_time', {})}{tc('read_emails', {'limit': 3})}"},
        {"role": "tool", "name": "get_system_time", "content": "14:05:00 22/04"},
        {"role": "tool", "name": "read_emails", "content": "--- Email 1 ---\nTừ: AWS\nTiêu đề: URGENT Billing"},
        {"role": "assistant", "content": f"Thời gian hiện tại là 14:05. Có mail AWS khẩn nên em đẩy Zalo qua liền.{tc('send_zalo_bot', {'message': '🚨 Anh Dương ơi 14:05 rồi, AWS tự nhiên bắt đầu tính phí Billing URGENT nè 💸'})}"},
        {"role": "tool", "name": "send_zalo_bot", "content": "Success"},
        {"role": "assistant", "content": "Mọi thao tác đã xử lý mượt mà báo lên Zalo xong ạ!"}
    ]
    add_to_dataset(msgs, "hard", 3)


# 4. FAKE TOOL & OUT OF SCOPE
for _ in range(250):
    u = random.choice([
        "Lên Google Search Drive tìm file report.xlsx cho anh.",
        "Xài hệ thống search Drive lấy hợp đồng cũ ra.",
        "Em có thấy File kế toán trong search drive không?" 
    ]) + f" (id: {random.randint(1,999)})"
    
    msgs = [
        {"role": "system", "content": get_dynamic_system_prompt()},
        {"role": "user", "content": u},
        {"role": "assistant", "content": "Dạ em rất tiếc! Mặc dù em có tool search_drive nhưng công cụ này đang bị hỏng và không sử dụng được ạ. Anh có cần em kiểm tra Mail hoặc báo Zalo không?"}
    ]
    add_to_dataset(msgs, "medium", 0)

time_standalone = ["giờ rứa mậy?", "Nhìn đồng hồ xem", "bây h là mấy h?", "Current time?"]
for _ in range(300):
    msgs = [
        {"role": "system", "content": get_dynamic_system_prompt()},
        {"role": "user", "content": random.choice(time_standalone) + f" (id: {random.randint(1,999)})"},
        {"role": "assistant", "content": f"Dạ để em lướt mắt qua đồng hồ.{tc('get_system_time', {})}"},
        {"role": "tool", "name": "get_system_time", "content": "11:15:20"},
        {"role": "assistant", "content": "Bây giờ đang là 11:15 anh Dương nhé."}
    ]
    add_to_dataset(msgs, "easy", 1)


# 5. SPLIT 80/20 & EXPORT
random.shuffle(dataset)

split_idx = int(len(dataset) * 0.8)
train_dataset = dataset[:split_idx]
valid_dataset = dataset[split_idx:]

with open('train_zalo_tool.jsonl', 'w', encoding='utf-8') as f:
    for d in train_dataset:
        f.write(json.dumps(d, ensure_ascii=False) + '\n')

with open('validation_zalo_tool.jsonl', 'w', encoding='utf-8') as f:
    for d in valid_dataset:
        f.write(json.dumps(d, ensure_ascii=False) + '\n')

print(f"Created V13 MASTERY Datasets:")
print(f" - Train: {len(train_dataset)} samples")
print(f" - Valid: {len(valid_dataset)} samples")
print(f" - Total Unique V13: {len(dataset)} samples")
