import json
import random
from datetime import datetime

# Define the system prompt with tool instructions exactly like Qwen2.5/LIVA
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

# ---------------------------------------------------------
# 1. POSITIVE MAPPING (Read Emails -> Send Zalo)
# ---------------------------------------------------------
pos_prompts = [
    "Giúp tôi tóm tắt {n} gmail gần nhất rồi gửi qua zalo nhé.",
    "Bạn đọc {n} email mới nhất, tóm gọn lại rồi nhắn qua Zalo bot cho tôi.",
    "Check hộp thư lấy {n} mail mới, tóm tắt ý chính rồi chuyển tiếp nội dung qua Zalo nha Liva.",
    "Có email nào mới không, lấy {n} cái tóm tắt rồi gửi Zalo đi.",
    "Liva ơi, anh nhờ xíu. Đọc giùm {n} cái email vừa đến xong push thẳng qua tài khoản Zalo giúp anh nhé.",
    "Kiểm tra hòm thư xem có {n} cái mail nào quan trọng không, tóm tắt lẹ gửi Zalo cho mình.",
    "Đọc {n} email gần đây nhất, viết gọn lại và báo cáo qua Zalo Bot.",
    "Giúp tôi tóm tắt những mail quan trọng trong ngày hôm nay rồi gửi Zalo.",
    "Hôm nay có email nào mới đến không, lọc rác đi rồi gửi vào Zalo cho tôi nhé.",
    "Check mail ngày hôm nay giúp anh, cái nào quan trọng thì ping Zalo nha.",
    "Vào đọc email mới gửi đến, tóm tắt lại ý chính và bắn qua Zalo.",
    "liva ơi tóm tắc giùm {n} cái meo nha",
    "Ê liva tóm tắt mấy thư hnay r xend zalo hộ tui",
    "xem hòm thư có zì hay k, tóm tất qua zlo cho xép đi",
    "mày doc mail hom nay roi tom luoc gui zalo gium tao voi nha",
    "Liva tom tat nhung email qtrong giup a rui pắn sang zàlo nhé"
]

email_templates = [
    # 1. Standard Important
    ("--- Email 1 ---\nTừ: HR Bộ phận IT\nTiêu đề: Thông báo lịch phỏng vấn\n--- Email {n} ---\nTừ: GitHub\nTiêu đề: Action failed on main branch", 
     "📍 HR báo có lịch phỏng vấn mới.\\n⚠️ GitHub báo lỗi build trên nhánh main anh nhé.",
     "Dạ, em đã đọc {n} email mới nhất. Em thấy cả 2 luồng công việc này đều khẩn cấp nên em vừa tổng hợp và gửi thẳng nội dung qua Zalo cho anh rồi ạ."),
    
    # 2. Spam Drop
    ("--- Email 1 ---\nTừ: Shopee\nTiêu đề: Đơn hàng đã giao thành công\n--- Email {n} ---\nTừ: Nguyễn Văn A\nTiêu đề: Tài liệu dự án LIVA", 
     "Nguyễn Văn A vừa gửi tài liệu dự án LIVA cho anh nha.",
     "Chào anh Dương, có email gửi tài liệu LIVA từ anh Nguyễn Văn A em đã chuyển qua Zalo. Còn cái mail báo giao hàng của Shopee em coi là rác nên đã chủ động gạch đi để đỡ phiền anh ạ."),
    
    # 3. Urgent English & Spam
    ("--- Email 1 ---\nTừ: Apple\nTiêu đề: Your iCloud Receipt\n--- Email {n} ---\nTừ: AWS Support\nTiêu đề: Action Required: Instance retiring", 
     "🚨 Cảnh báo AWS: Cloud Instance đang sắp bị retire, cần vào kiểm tra gấp.",
     "Dạ em kiểm tra thì thấy có 1 cảnh báo khẩn từ AWS em vừa ping qua Zalo cho anh rùi. Cái hóa đơn iCloud của Apple thì em không đưa vào vì nó gửi định kỳ đóng tiền thôi ạ."),
     
    # 4. Standard Task
    ("--- Email 1 ---\nTừ: DuongNAD\nTiêu đề: Xin nghỉ phép ngày mai\n--- Email {n} ---\nTừ: Jira\nTiêu đề: Có 2 task mới được gán cho bạn", 
     "1. DuongNAD xin nghỉ phép ngày mai.\\n2. Anh vừa được assign 2 task mới trên Jira.",
     "Đã xử lý xong tác vụ: Nhận thấy cả thư xin nghỉ phép của nhân viên và thông báo task mới đều đáng chú ý nên em đã tóm tắt đủ và nhắn Zalo cho anh ạ."),
     
    # 5. Urgent Security & Spam
    ("--- Email 1 ---\nTừ: Steam\nTiêu đề: Lượt sale đặc biệt mùa xuân\n--- Email {n} ---\nTừ: Vietcombank\nTiêu đề: Cảnh báo đăng nhập thiết bị lạ", 
     "🔥 Vietcombank cảnh báo có người vừa đăng nhập tài khoản bằng thiết bị lạ!",
     "Anh ơi có biến, Vietcombank vừa gửi mail cảnh báo bảo mật, em đã dội ngay qua Zalo cho anh rồi đấy! Còn cái mail quảng cáo Sale của Steam thì em sút đi rồi nha."),

    # 6. Multi-lingual Mixed
    ("--- Email 1 ---\nTừ: Client_Takashi\nTiêu đề: Urgent update required for the frontend\n--- Email {n} ---\nTừ: Lazada\nTiêu đề: Tặng bạn Voucher 50K", 
     "Khách hàng Takashi yêu cầu update gấp cho phần Frontend.",
     "Dạ em thấy có email giục việc từ anh Takashi, em đã note lại trên Zalo. Còn ba cái voucher lẻ tẻ của Lazada em tự động ẩn đi rồi ạ."),
     
    # 7. Heavy Tech context
    ("--- Email 1 ---\nTừ: Sentry.io\nTiêu đề: [New Issue] TypeError: null is not an object\n--- Email {n} ---\nTừ: DevOps Team\nTiêu đề: Server MongoDB đang spike CPU 90%", 
     "🔥 Sentry: Bug mới TypeError null object.\\n🔥 DevOps: CPU MongoDB đang vọt lên 90%.",
     "Tình hình khá căng anh ạ, Sentry đang báo bug và server DB CPU đang nhảy vọt, em đã gửi thông báo nhanh qua Zalo để anh cân nhắc fix gấp nhé."),

    # 8. Relaxed Context
    ("--- Email 1 ---\nTừ: NhanSu\nTiêu đề: Thông báo tiệc tất niên cuối năm\n--- Email {n} ---\nTừ: Spotify\nTiêu đề: Nghe lại những bài hát bạn yêu thích", 
     "Nhân sự thông báo lịch tổ chức tiệc tất niên của công ty.",
     "Trong {n} email vừa đến, em chỉ nhặt lại cái thông báo tiệc Tất niên của công ty để gửi anh, còn mail gợi ý nhạc của Spotify thì em skip nha anh."),

    # 9. Personal Work
    ("--- Email 1 ---\nTừ: Accountant\nTiêu đề: Yêu cầu cập nhật chứng từ nộp thuế\n--- Email {n} ---\nTừ: Google Alerts\nTiêu đề: Bạn có 3 tin tức mới về AI", 
     "Kế toán đang giục anh nộp chứng từ thuế kìa.",
     "Có email kế toán đòi giấy nộp thuế em vừa forward qua Zalo đó. Mấy cái bản tin AI buổi sáng từ Google Alerts em tạm thời chưa lấy vào để anh tập trung xử lý việc trước."),

    # 10. Direct Questioning
    ("--- Email 1 ---\nTừ: Zalo Team\nTiêu đề: Update chính sách ZaloOA\n--- Email {n} ---\nTừ: Marketing\nTiêu đề: Gửi duyệt banner chạy quảng cáo", 
     "1. Zalo vừa đổi chính sách ZaloOA.\\n2. Ban Marketing xin duyệt banner mới.",
     "Em hoàn thành tóm tắt rồi ạ. Cả chính sách mới của Zalo và yêu cầu duyệt banner em đều thấy cần thiết nên đã gửi liệt kê chi tiết bên Zalo Bot cho anh.")
]

# ---------------------------------------------------------
# 2. NEGATIVE MAPPING (Irrelevant/Refusals/Other tools)
# Để model không bị "nghiện" gửi Zalo vô tội vạ
# ---------------------------------------------------------
neg_prompts = [
    ("Bây giờ là mấy giờ rồi Liva?", "get_system_time", "{}"),
    ("Mấy giờ cmnr?", "get_system_time", "{}"),
    ("Thế giới hôm nay có tin tức gì hot không?", None, "Dạ em xin lỗi, em hiện chưa được trang bị kỹ năng hoặc công cụ để tra cứu tin tức thế giới ạ."),
    ("Tắt máy tính giúp anh.", None, "Dạ, hiện tại LIVA không có quyền hệ thống để can thiệp vào máy tính cục bộ của anh ạ."),
    ("Bạn có thể viết cho tôi một bài thơ về biển được không?", None, "Dạ được ạ. *Gió thổi miên man trên bãi biển dài...* (Em tự sáng tác chay vì tác vụ này không cần dùng Zalo hay đọc mail ạ)."),
    ("Hãy đọc 2 mail gần nhất nhưng đừng gửi đi đâu cả, chỉ hiển thị ở đây thôi.", "read_emails", "{\"limit\": 2}"),
    ("thời tiết hà nội hnay mếy độ z Liva?", None, "Dạ em chưa được cài đặt công cụ xem thời tiết, anh thử hỏi trên Google xem sao nhé."),
    ("tư vấn cho tui cách fix bug React", None, "Dạ, để sửa lỗi React, anh có thể cung cấp thêm chi tiết đoạn code bị lỗi để em phân tích trực tiếp cho anh nhé."),
    ("hát 1 bài đi", None, "La là la lá 🎵"),
    ("viết cho tao đoạn code python in ra chữ hello", None, "Dạ đây là đoạn code anh cần:\n```python\nprint('hello')\n```"),
    ("có cần gửi zalo không mậy?", None, "Dạ em chỉ gửi Zalo khi nào anh yêu cầu đọc và tóm tắt thư thôi ạ. Hiện tại anh không yêu cầu nên em phản hồi bằng text bình thường ạ.")
]

dataset = []

# Generate 1500 Multi-step Positive Examples (Email -> Zalo)
for i in range(1500):
    n = random.randint(1, 10)
    user_msg = random.choice(pos_prompts).format(n=n)
    
    email_tuple = random.choice(email_templates)
    email_mock = email_tuple[0].format(n=n)
    safe_summary = email_tuple[1] # Removed the generic .format(n=n) trick
    final_AI_speech = email_tuple[2].replace("{n}", str(n))
    
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": user_msg},
        {
            "role": "assistant",
            "tool_calls": {"name": "read_emails", "arguments": f"{{\"limit\": {n}}}"}
        },
        {
            "role": "tool",
            "name": "read_emails",
            "content": email_mock
        },
        {
            "role": "assistant",
            "content": final_AI_speech,
            "tool_calls": {"name": "send_zalo_bot", "arguments": f"{{\"message\": \"{safe_summary}\"}}"}
        },
        {
            "role": "tool",
            "name": "send_zalo_bot",
            "content": "Hoàn tất (Success): Đã gửi tin nhắn thành công qua Zalo Bot Creator."
        }
    ]
    dataset.append({"messages": messages})

# Generate 500 Negative/Single-tool Examples to prevent "Zalo-spamming" hallucination
for i in range(500):
    item = random.choice(neg_prompts)
    user_msg = item[0]
    tool_name = item[1]
    args = item[2]
    
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": user_msg}
    ]
    
    if tool_name is None:
        messages.append({"role": "assistant", "content": args})
    else:
        # It's a single tool call request (Time or Just reading emails without Zalo)
        messages.append({
            "role": "assistant",
            "tool_calls": {"name": tool_name, "arguments": args}
        })
        if tool_name == "get_system_time":
            messages.append({
                "role": "tool",
                "name": "get_system_time",
                "content": "15:35:00 15/03/2026"
            })
            messages.append({"role": "assistant", "content": "Bây giờ là 15 giờ 35 phút ngày 15/03/2026 ạ."})
        elif tool_name == "read_emails":
            messages.append({
                "role": "tool",
                "name": "read_emails",
                "content": "--- Email 1 ---\\nTừ: Sếp\\nTiêu đề: Chấm điểm KPI\\n--- Email 2 ---\\nTừ: LIVA Team\\nTiêu đề: Ra mắt bản V2"
            })
            messages.append({"role": "assistant", "content": "Dạ, anh có 2 thư mới. Một cái từ Sếp về chấm điểm KPI, một cái từ Team LIVA báo ra mắt bản V2 ạ."})
            
    dataset.append({"messages": messages})

# ---------------------------------------------------------
# 2.5 EDGE CASES (Empty Inbox)
# ---------------------------------------------------------
for i in range(200):
    user_msg = random.choice(pos_prompts).format(n=random.randint(1,10))
    # AI checks email, but API returns nothing
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": user_msg},
        {
            "role": "assistant",
            "tool_calls": {"name": "read_emails", "arguments": '{"limit": 5}'}
        },
        {
            "role": "tool",
            "name": "read_emails",
            "content": "[] Hoặc Không tìm thấy email mới nào."
        },
        {
            "role": "assistant",
            "content": "Dạ thưa Anh Dương, hiện tại hòm thư của anh không có email nào mới để tóm tắt ạ."
        }
    ]
    dataset.append({"messages": messages})

# ---------------------------------------------------------
# 2.6 ADVANCED EDGE CASES (API Failures)
# ---------------------------------------------------------
for i in range(100):
    user_msg = random.choice(pos_prompts).format(n=random.randint(1,5))
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": user_msg},
        {
            "role": "assistant",
            "tool_calls": {"name": "read_emails", "arguments": '{"limit": 2}'}
        },
        {
            "role": "tool",
            "name": "read_emails",
            "content": "--- Email 1 ---\\nTừ: Sếp\\nTiêu đề: Họp gấp dự án mới\\n--- Email 2 ---\\nTừ: NS\\nTiêu đề: Bảng lương tháng này"
        },
        {
            "role": "assistant",
            "content": "Dạ em đã đọc mail xong rồi, để em gửi tóm tắt qua Zalo cho anh nhé.",
            "tool_calls": {"name": "send_zalo_bot", "arguments": '{"message": "1. Sếp: Họp gấp dự án mới.\\n2. NS: Bảng lương tháng này."}'}
        },
        {
            "role": "tool",
            "name": "send_zalo_bot",
            "content": "Error 500: Zalo API Endpoint Timeout or Blocked."
        },
        {
            "role": "assistant",
            "content": "Dạ em đã tóm tắt xong (1. Họp dự án, 2. Báo lương), nhưng Zalo API của anh hiện đang bị lỗi kết nối không gửi được ạ. Anh có muốn đọc tạm ở đây không?"
        }
    ]
    dataset.append({"messages": messages})

# ---------------------------------------------------------
# 2.7 MULTI-TURN CONVERSATIONS (Context Retention)
# ---------------------------------------------------------
for i in range(200):
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": "Tóm tắt mail hộ anh với Liva ơi."},
        {
            "role": "assistant",
            "tool_calls": {"name": "read_emails", "arguments": '{"limit": 5}'}
        },
        {
            "role": "tool",
            "name": "read_emails",
            "content": "--- Email 1 ---\\nTừ: Đối tác\\nTiêu đề: Phụ lục Hợp đồng CyberSentinel\\n--- Email 2 ---\\nTừ: Shopee\\nTiêu đề: Sale sốc mùng 3 tháng 3"
        },
        {
            "role": "assistant",
            "content": "Dạ, anh có 2 thư. 1 cái rác Shopee em bỏ qua. Cái Hợp đồng CyberSentinel thì em gửi qua Zalo liền đây.",
            "tool_calls": {"name": "send_zalo_bot", "arguments": '{"message": "Đối tác vừa gửi Phụ lục hợp đồng CyberSentinel."}'}
        },
        {
            "role": "tool",
            "name": "send_zalo_bot",
            "content": "Success"
        },
        {
            "role": "assistant",
            "content": "Em đã gửi Zalo tóm tắt hợp đồng CyberSentinel cho anh Dương thành công rồi ạ!"
        },
        {"role": "user", "content": "Tuyệt lắm em, nãy em bảo bỏ qua mail gì đấy?"},
        {"role": "assistant", "content": "Dạ thư em bỏ qua là mail quảng cáo rác từ Shopee ạ. Em thấy nó không quan trọng bằng công việc nên em đã tự gạch bỏ để tránh phiền anh."}
    ]
    dataset.append({"messages": messages})

# ---------------------------------------------------------
# 2.8 COMPLEX QUERIES (Condition Filtering)
# ---------------------------------------------------------
complex_prompts = [
    ("Liva ơi, check xem Sếp có gửi mail nào trong 20 cái gần nhất không, có thì tóm tắt gửi Zalo cho anh.", "Sếp"),
    ("Tìm trong 15 mail mới nhất xem có cái nào nhắc đến từ 'Hợp đồng' không rồi bắn Zalo giúp mình.", "Hợp đồng"),
    ("Lọc trong hộp thư lấy 10 mail, chỉ lấy mail có file đính kèm (attachment) hoặc cờ ưu tiên (priority flag) rồi gửi Zalo nhé.", "Attachment/Priority")
]

for i in range(100):
    user_msg, condition = random.choice(complex_prompts)
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": user_msg},
        {
            "role": "assistant",
            "tool_calls": {"name": "read_emails", "arguments": '{"limit": 20}'}
        },
        {
            "role": "tool",
            "name": "read_emails",
            "content": "--- Email 1 ---\\nTừ: Sếp\\nTiêu đề: [URGENT] Báo cáo quý 3\\nĐính kèm: report.pdf\\n--- Email 2 ---\\nTừ: Kế toán\\nTiêu đề: Hợp đồng cho thuê văn phòng"
        },
        {
            "role": "assistant",
            "content": "Dạ em đã kiểm tra và tìm thấy đúng những email anh cần. Em sẽ gửi tóm tắt qua Zalo ngay lập tức ạ.",
            "tool_calls": {"name": "send_zalo_bot", "arguments": '{"message": "1. Sếp: Báo cáo quý 3 (Có đính kèm).\\n2. Kế toán: Hợp đồng thuê văn phòng."}'}
        },
        {
            "role": "tool",
            "name": "send_zalo_bot",
            "content": "Hoàn tất (Success): Đã gửi tin nhắn rỗng"
        },
        {
            "role": "assistant",
            "content": "Em đã hoàn tất việc lọc email theo yêu cầu khắt khe của anh và gửi Zalo thành công rồi nhé!"
        }
    ]
    dataset.append({"messages": messages})

# ---------------------------------------------------------
# 2.9 PARALLEL TOOL CALLS (Read Emails + Time)
# ---------------------------------------------------------
for i in range(100):
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": "Mấy giờ rồi Liva? À nhân tiện đọc cho anh 5 cái mail mới luôn xem có gì hot không rồi gửi Zalo đi."},
        {
            "role": "assistant",
            "tool_calls": [
                {"name": "get_system_time", "arguments": "{}"},
                {"name": "read_emails", "arguments": '{"limit": 5}'}
            ]
        },
        {
            "role": "tool",
            "name": "get_system_time",
            "content": "15:35:00 15/03/2026"
        },
        {
            "role": "tool",
            "name": "read_emails",
            "content": "--- Email 1 ---\\nTừ: Cloudflare\\nTiêu đề: Domain expiring soon"
        },
        {
            "role": "assistant",
            "content": "Dạ hiện tại là 15 giờ 35 phút ngày 15/03/2026 ạ. Em cũng thấy có 1 mail từ Cloudflare báo tên miền sắp hết hạn, để em gửi thông báo qua Zalo cho anh nhé.",
            "tool_calls": {"name": "send_zalo_bot", "arguments": '{"message": "Cloudflare báo tên miền sắp hết hạn (cập nhật lúc 15:35)."}'}
        },
        {
            "role": "tool",
            "name": "send_zalo_bot",
            "content": "Success"
        },
        {
            "role": "assistant",
            "content": "Zalo đã được báo tin thành công ạ!"
        }
    ]
    dataset.append({"messages": messages})

# ---------------------------------------------------------
# 2.10 ADVANCED NEGATIVE EXAMPLES (Tool Not Exists)
# ---------------------------------------------------------
for i in range(50):
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": "Liva ơi, gọi get_weather xem thời tiết hôm nay thế nào."},
        {
            "role": "assistant",
            "content": "Dạ em xin lỗi, hiện tại em chưa được cung cấp công cụ dự báo thời tiết (get_weather). Em chỉ có khả năng duyệt email và gửi tin nhắn Zalo thôi ạ."
        }
    ]
    dataset.append({"messages": messages})

# ---------------------------------------------------------
# 2.11 EXTENDED MULTI-TURN (Deep context)
# ---------------------------------------------------------
for i in range(50):
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": "Check mail đi Liva."},
        {
            "role": "assistant",
            "tool_calls": {"name": "read_emails", "arguments": '{"limit": 5}'}
        },
        {
            "role": "tool",
            "name": "read_emails",
            "content": "--- Email 1 ---\\nTừ: IT Dept\\nTiêu đề: Cập nhật Mật khẩu\\n--- Email 2 ---\\nTừ: Steam\\nTiêu đề: Sale"
        },
        {
            "role": "assistant",
            "content": "Dạ em tóm tắt thư mật khẩu quan trọng gửi Zalo anh nhé.",
            "tool_calls": {"name": "send_zalo_bot", "arguments": '{"message": "IT Dept yêu cầu cập nhật mật khẩu."}'}
        },
        {
            "role": "tool",
            "name": "send_zalo_bot",
            "content": "Success"
        },
        {
            "role": "assistant",
            "content": "Trình tự đã xong anh ạ."
        },
        {"role": "user", "content": "Thế cái email tóm tắt bảo quên mật khẩu là từ bộ phận nào gửi thế, IT hay là HR?"},
        {"role": "assistant", "content": "Dạ theo như email em vừa đọc lúc nãy, yêu cầu cập nhật mật khẩu là từ bộ phận IT (IT Dept) chứ không phải HR đâu anh."}
    ]
    dataset.append({"messages": messages})



# ---------------------------------------------------------
# 3. SHAREGPT CONVERSATIONAL INJECTION
# ---------------------------------------------------------
try:
    with open('openclaw_sharegpt_format.jsonl', 'r', encoding='utf-8') as sf:
        sharegpt_lines = sf.readlines()
        
    # Lấy 1000 mẫu ngẫu nhiên từ ShareGPT để không làm loãng quá data Zalo
    random.shuffle(sharegpt_lines)
    sharegpt_sample = sharegpt_lines[:1000]
    
    for line in sharegpt_sample:
        if not line.strip(): continue
        item = json.loads(line)
        convs = item.get("conversations", [])
        
        messages_converted = []
        has_system = False
        
        for turn in convs:
            role = turn.get("from", "").lower()
            content = turn.get("value", "")
            
            if role == "system":
                # Đè System Prompt tĩnh của ShareGPT bằng System Prompt động (có Tool Schema) của LIVA
                messages_converted.append({"role": "system", "content": SYSTEM_PROMPT})
                has_system = True
            elif role in ["human", "user"]:
                messages_converted.append({"role": "user", "content": content})
            elif role in ["gpt", "assistant"]:
                messages_converted.append({"role": "assistant", "content": content})
                
        if not has_system:
             messages_converted.insert(0, {"role": "system", "content": SYSTEM_PROMPT})
             
        # Chỉ lấy các chuỗi hội thoại có ý nghĩa
        if len(messages_converted) >= 2:
            dataset.append({"messages": messages_converted})
            
    print(f"Đã merge thành công {len(sharegpt_sample)} mẫu hội thoại ShareGPT để chống Overfit!")
except Exception as e:
    print(f"Bỏ qua gộp data ShareGPT vì lỗi: {e}")

# Shuffle the combined dataset perfectly to avoid sequential learning bias
random.shuffle(dataset)

with open('zalo_focused_tool_calling.jsonl', 'w', encoding='utf-8') as f:
    for item in dataset:
        f.write(json.dumps(item, ensure_ascii=False) + '\n')

print(f"Đã tạo hoàn thiện Dataset SIÊU KHỦNG V5 với {len(dataset)} mẫu.")
print("Bao gồm: Luồng Zalo (Positive) + Luồng Nhiễu (Negative) + Hội thoại mở rộng (ShareGPT).")
