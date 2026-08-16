import asyncio
import os
from dotenv import load_dotenv
from google.antigravity import Agent, LocalAgentConfig, CapabilitiesConfig

# Nạp biến môi trường từ file .env (để lấy GEMINI_API_KEY)
load_dotenv(os.path.join(os.path.dirname(__file__), "..", ".env"))

# Mock tool functions to simulate LIVA skills
def control_volume(action: str, steps: int = 1):
    """
    Controls the system volume.
    Args:
        action: 'up', 'down', or 'mute'
        steps: number of steps to adjust (default: 1)
    """
    return f"Volume {action} by {steps} steps."

def get_weather_info(location: str):
    """
    Gets the current weather for a specific location.
    Args:
        location: The city or location name.
    """
    return f"Weather info for {location}"

# Danh sách các bài test
TEST_CASES = [
    {
        "name": "Test kỹ năng điều khiển Volume",
        "prompt": "Vặn nhỏ nhạc xuống giúp tôi khoảng 2 nấc",
        "expected_tool": "control_volume"
    },
    {
        "name": "Test kỹ năng thời tiết",
        "prompt": "Trời Hà Nội hôm nay có mưa không?",
        "expected_tool": "get_weather_info"
    }
]

async def run_tests():
    # Khởi tạo CapabilitiesConfig (mặc định có sẵn một số built-in tool)
    capabilities = CapabilitiesConfig()

    # Cấu hình Agent với các skill/tool mà bạn đang phát triển
    config = LocalAgentConfig(
        system_instructions="Bạn là trợ lý ảo. Hãy quyết định xem nên gọi công cụ nào dựa trên yêu cầu của người dùng. Trả về gọi công cụ chứ không trả lời suông.",
        capabilities=capabilities,
    )

    async with Agent(config) as agent:
        for case in TEST_CASES:
            print(f"▶ Đang chạy: {case['name']}...")
            response = await agent.chat(case['prompt'])
            
            # Lấy danh sách các công cụ mà AI đã quyết định gọi
            called_tools = [tool.name async for tool in response.tool_calls]
            
            # Kiểm chứng (Assert)
            if case['expected_tool'] in called_tools:
                print("  ✅ PASS: AI đã gọi đúng skill!")
            else:
                print(f"  ❌ FAILED: AI không gọi {case['expected_tool']}, mà lại gọi {called_tools}")

if __name__ == "__main__":
    asyncio.run(run_tests())
