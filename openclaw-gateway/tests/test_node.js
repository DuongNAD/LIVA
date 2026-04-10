const { OpenAI } = require('openai');

const aiClient = new OpenAI({
    baseURL: 'http://127.0.0.1:8000/v1',
    apiKey: 'sk-no-key-required'
});

async function test() {
    const response = await aiClient.chat.completions.create({
        model: "local-model",
        messages: [
            { 
                role: "system", 
                content: `Bạn là Liva, một trợ lý AI thông minh.\n\nHƯỚNG DẪN DÙNG KỸ NĂNG:\nBạn có quyền truy cập vào các công cụ sau. Nếu yêu cầu cần dùng công cụ, hãy phản hồi bằng JSON gọi hàm. NẾU thiếu tham số, hãy hỏi lại người dùng. NẾU không có công cụ phù hợp, hãy từ chối.\n[\n  {\n    "name": "read_emails",\n    "description": "Đọc email từ hòm thư.",\n    "parameters": {\n      "type": "object",\n      "properties": {\n        "limit": {"type": "number", "description": "Số lượng email"}\n      },\n      "required": ["limit"]\n    }\n  }\n]`
            },
            { role: "user", content: "Giúp tôi tóm tắt 5 gmail gần nhất" }
        ],
        temperature: 0.3,
        max_tokens: 500
    });
    console.log(JSON.stringify(response, null, 2));
}

test().catch(console.error);
