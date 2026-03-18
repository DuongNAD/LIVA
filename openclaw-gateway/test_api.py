import requests
import json

url = "http://127.0.0.1:8000/v1/chat/completions"

payload = {
    "model": "local-model",
    "messages": [
        {"role": "system", "content": "Bạn là mạng AI Liva. HƯỚNG DẪN DÙNG KỸ NĂNG: Nếu cần thiết hãy gọi tool_calls."},
        {"role": "user", "content": "Xem thời tiết Hà Nội"}
    ],
    "tools": [
        {
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Lấy thời tiết",
                "parameters": {"type": "object", "properties": {"location": {"type": "string"}}}
            }
        }
    ],
    "tool_choice": "auto",
    "max_tokens": 200
}

response = requests.post(url, json=payload)
print(json.dumps(response.json(), indent=2, ensure_ascii=False))
