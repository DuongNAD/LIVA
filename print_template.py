from transformers import AutoTokenizer

tokenizer = AutoTokenizer.from_pretrained("Qwen/Qwen2.5-7B-Instruct")
messages = [
    {"role": "system", "content": "Bạn là Liva.\n\nHƯỚNG DẪN DÙNG KỸ NĂNG:\nBạn có quyền truy cập vào các công cụ..."},
    {"role": "user", "content": "Theo dõi thời tiết"},
    {"role": "assistant", "tool_calls": [{"id": "call_123", "type": "function", "function": {"name": "get_weather", "arguments": "{\"location\": \"Hanoi\"}"}}]}
]

result = tokenizer.apply_chat_template(messages, tokenize=False, add_generation_prompt=False)
print("================ EXPECTED TEMPLATE ================")
print(result)
print("===================================================")
