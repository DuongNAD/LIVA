#!/usr/bin/env python3
"""
LIVA Voice Tester — Kiểm tra các giọng nữ Edge-TTS
Tạo file sample cho từng giọng để so sánh chất lượng.
"""

import asyncio
import os
import sys
import logging

logging.basicConfig(level=logging.INFO, format="[VoiceTest] %(levelname)s: %(message)s")
logger = logging.getLogger("voice_test")

# Danh sách giọng nữ để test
FEMALE_VOICES = [
    # Vietnamese
    {"id": "vi-VN-HoaiMyNeural", "name": "Hoài My (Vietnamese)", "lang": "vi-VN", "text": "Xin chào, tôi là trợ lý ảo Liva. Rất vui được gặp bạn hôm nay. Tôi có thể giúp gì cho bạn?"},
    # English (top picks)
    {"id": "en-US-AvaMultilingualNeural", "name": "Ava (US Multilingual)", "lang": "en-US", "text": "Hello, I am LIVA, your virtual assistant. How can I help you today?"},
    {"id": "en-US-AriaNeural", "name": "Aria (US News)", "lang": "en-US", "text": "Hello, I am LIVA, your virtual assistant. How can I help you today?"},
    {"id": "en-US-JennyNeural", "name": "Jenny (US General)", "lang": "en-US", "text": "Hello, I am LIVA, your virtual assistant. How can I help you today?"},
    # Japanese
    {"id": "ja-JP-NanamiNeural", "name": "Nanami (Japanese)", "lang": "ja-JP", "text": "こんにちは、私はLIVAです。今日はどんなお手伝いができますか？"},
    # Korean
    {"id": "ko-KR-SunHiNeural", "name": "SunHi (Korean)", "lang": "ko-KR", "text": "안녕하세요, 저는 LIVA입니다. 오늘 무엇을 도와드릴까요?"},
    # Chinese
    {"id": "zh-CN-XiaoxiaoNeural", "name": "Xiaoxiao (Chinese)", "lang": "zh-CN", "text": "你好，我是LIVA，你的虚拟助手。今天有什么可以帮助你的？"},
]


async def test_single_voice(voice_info: dict, output_dir: str) -> bool:
    """Test 1 giọng và lưu file MP3"""
    try:
        import edge_tts

        voice_id = voice_info["id"]
        output_file = os.path.join(output_dir, f"sample_{voice_id}.mp3")

        logger.info(f"🎤 Testing: {voice_info['name']} ({voice_id})")
        logger.info(f"   Text: {voice_info['text'][:60]}...")

        communicate = edge_tts.Communicate(
            voice_info["text"],
            voice_id,
            rate="+10%"
        )

        audio_data = bytearray()
        async for chunk in communicate.stream():
            if chunk["type"] == "audio":
                audio_data.extend(chunk["data"])

        if len(audio_data) > 0:
            with open(output_file, "wb") as f:
                f.write(audio_data)
            size_kb = len(audio_data) / 1024
            logger.info(f"   ✅ Saved: {output_file} ({size_kb:.1f} KB)")
            return True
        else:
            logger.warning(f"   ❌ No audio data generated for {voice_id}")
            return False

    except Exception as e:
        logger.error(f"   ❌ Error testing {voice_info['id']}: {e}")
        return False


async def main():
    """Main test routine"""
    print("\n" + "=" * 60)
    print("🎙️  LIVA Voice Tester — Female Voice Samples")
    print("=" * 60 + "\n")

    # Tạo thư mục output
    output_dir = os.path.join(os.path.dirname(__file__), "workspace", "voice_samples")
    os.makedirs(output_dir, exist_ok=True)

    results = []
    for voice in FEMALE_VOICES:
        ok = await test_single_voice(voice, output_dir)
        results.append({"voice": voice, "ok": ok})
        print()

    # Summary
    print("\n" + "=" * 60)
    print("📊 SUMMARY")
    print("=" * 60)
    success = sum(1 for r in results if r["ok"])
    print(f"   ✅ Success: {success}/{len(results)}")
    print(f"   📁 Output: {output_dir}")
    print()

    for r in results:
        status = "✅" if r["ok"] else "❌"
        print(f"   {status} {r['voice']['name']} ({r['voice']['id']})")

    print("\n" + "=" * 60)
    print("💡 Mở file MP3 trong thư mục trên để nghe và chọn giọng phù hợp!")
    print("=" * 60 + "\n")


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    asyncio.run(main())
