/**
 * Provides few-shot XML tool calling examples adapted to user language.
 * Extracted from PromptBuilder for clean Separation of Concerns.
 */
export function getFewShotExamples(userLang: string): string {
    const isVietnamese = (userLang || "").toLowerCase().startsWith("vi");

    if (isVietnamese) {
        return `User: "nhắn tin cho bạn Khánh trên messenger hỏi xem nó ngủ chưa"
Correct response:
<tool_call>
{"name": "send_messenger_rpa", "arguments": {"targetName": "Khánh", "message": "Khánh ơi ngủ chưa vậy?"}}
</tool_call>

User: "nhắn tin cho Mẹ trên zalo bảo con về muộn"
Correct response:
<tool_call>
{"name": "send_zalo_rpa", "arguments": {"targetName": "Mẹ", "message": "Mẹ ơi hôm nay con về muộn chút nha mẹ"}}
</tool_call>

User: "nhắn zalo cho Khánh hỏi mai học sáng hay chiều"
Correct response:
<tool_call>
{"name": "send_zalo_rpa", "arguments": {"targetName": "Khánh", "message": "Khánh ơi mai học sáng hay chiều vậy?"}}
</tool_call>

User: "thời tiết hôm nay thế nào"
Correct response:
<tool_call>
{"name": "get_weather_forecast", "arguments": {"days": 1}}
</tool_call>

User: "ngày mai có mưa không"
Correct response:
<tool_call>
{"name": "get_weather_forecast", "arguments": {"days": 2}}
</tool_call>

User: "dự báo thời tiết Đà Nẵng 3 ngày tới"
Correct response:
<tool_call>
{"name": "get_weather_forecast", "arguments": {"location": "Da Nang", "days": 3}}
</tool_call>

⚠️ ZALO ROUTING RULE: "nhắn zalo cho [TÊN NGƯỜI]" → ALWAYS use send_zalo_rpa (browser). send_zalo_bot is ONLY for sending reports/notifications to THE USER THEMSELVES, never for messaging friends.`;
    }

    return `User: "message Khanh on messenger to see if he's asleep"
Correct response:
<tool_call>
{"name": "send_messenger_rpa", "arguments": {"targetName": "Khanh", "message": "Khanh, are you asleep?"}}
</tool_call>

User: "message Mom on zalo saying I'll be home late"
Correct response:
<tool_call>
{"name": "send_zalo_rpa", "arguments": {"targetName": "Mom", "message": "Mom, I'll be home a bit late today"}}
</tool_call>

User: "zalo Khanh asking if we study morning or afternoon tomorrow"
Correct response:
<tool_call>
{"name": "send_zalo_rpa", "arguments": {"targetName": "Khanh", "message": "Khanh, do we study in the morning or afternoon tomorrow?"}}
</tool_call>

User: "how is the weather today"
Correct response:
<tool_call>
{"name": "get_weather_forecast", "arguments": {"days": 1}}
</tool_call>

User: "will it rain tomorrow"
Correct response:
<tool_call>
{"name": "get_weather_forecast", "arguments": {"days": 2}}
</tool_call>

User: "weather forecast for Da Nang for next 3 days"
Correct response:
<tool_call>
{"name": "get_weather_forecast", "arguments": {"location": "Da Nang", "days": 3}}
</tool_call>

⚠️ ZALO ROUTING RULE: "zalo [NAME]" → ALWAYS use send_zalo_rpa (browser). send_zalo_bot is ONLY for sending reports/notifications to THE USER THEMSELVES, never for messaging friends.`;
}
