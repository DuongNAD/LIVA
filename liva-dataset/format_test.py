import json
from transformers import AutoTokenizer

tokenizer = AutoTokenizer.from_pretrained("Qwen/Qwen2.5-7B-Instruct")

messages = [
    {
        "role": "system",
        "content": "Bạn là mạng AI.\n\nHƯỚNG DẪN DÙNG KỸ NĂNG:\nBạn có quyền truy cập vào các công cụ...\n[\n  {\n    \"name\": \"get_weather\"\n  }\n]"
    },
    {
        "role": "user",
        "content": "Hôm nay thời tiết thế nào?"
    },
    {
        "role": "assistant",
        "tool_calls": [
            {
                "id": "call_123",
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "arguments": "{\"loc\": \"Hanoi\"}"
                }
            }
        ]
    }
]

prompt = tokenizer.apply_chat_template(messages, tokenize=False, add_generation_prompt=False)
print("=== TEMPLATE BEGIN ===")
print(prompt)
print("=== TEMPLATE END ===")
