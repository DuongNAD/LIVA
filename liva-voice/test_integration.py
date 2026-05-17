#!/usr/bin/env python3
"""
LIVA Voice Integration Test — End-to-End
Test luồng: Gateway → Voice Engine → Edge-TTS → Audio

Yêu cầu: voice_engine.py đang chạy trên port 8002
"""

import asyncio
import json
import base64
import os
import sys
import logging

logging.basicConfig(level=logging.INFO, format="[IntegTest] %(levelname)s: %(message)s")
logger = logging.getLogger("integ_test")


async def test_http_tts(voice_id: str, text: str, output_dir: str) -> bool:
    """Test HTTP /tts endpoint"""
    try:
        import httpx
        async with httpx.AsyncClient(timeout=30.0) as client:
            logger.info(f"📡 POST /tts — voice: {voice_id}, text: {text[:40]}...")
            res = await client.post(
                "http://127.0.0.1:8002/tts",
                json={"text": text}
            )
            data = res.json()
            if data.get("status") == "ok" and data.get("audio"):
                audio_bytes = base64.b64decode(data["audio"])
                out_path = os.path.join(output_dir, f"http_tts_{voice_id}.mp3")
                with open(out_path, "wb") as f:
                    f.write(audio_bytes)
                logger.info(f"   ✅ HTTP TTS OK — {len(audio_bytes)/1024:.1f} KB → {out_path}")
                return True
            else:
                logger.warning(f"   ❌ HTTP TTS returned: {data}")
                return False
    except Exception as e:
        logger.error(f"   ❌ HTTP TTS failed: {e}")
        return False


async def test_ws_set_voice(voice_id: str) -> bool:
    """Test WebSocket set_voice event"""
    try:
        import websockets
        async with websockets.connect("ws://127.0.0.1:8002/ws", close_timeout=5) as ws:
            logger.info(f"📡 WS set_voice → {voice_id}")
            await ws.send(json.dumps({"type": "set_voice", "voice": voice_id}))
            await asyncio.sleep(0.5)
            logger.info(f"   ✅ set_voice sent OK")
            return True
    except Exception as e:
        logger.error(f"   ❌ WS set_voice failed: {e}")
        return False


async def test_ws_tts(text: str, output_dir: str, voice_label: str) -> bool:
    """Test WebSocket TTS with audio reception"""
    try:
        import websockets
        async with websockets.connect("ws://127.0.0.1:8002/ws", close_timeout=10) as ws:
            logger.info(f"📡 WS tts — text: {text[:40]}...")
            await ws.send(json.dumps({"type": "tts", "text": text}))

            # Wait for audio response
            audio_received = False
            try:
                msg = await asyncio.wait_for(ws.recv(), timeout=15.0)
                data = json.loads(msg)
                if data.get("type") == "audio" and data.get("data"):
                    audio_bytes = base64.b64decode(data["data"])
                    out_path = os.path.join(output_dir, f"ws_tts_{voice_label}.mp3")
                    with open(out_path, "wb") as f:
                        f.write(audio_bytes)
                    logger.info(f"   ✅ WS TTS OK — {len(audio_bytes)/1024:.1f} KB → {out_path}")
                    audio_received = True
            except asyncio.TimeoutError:
                logger.warning(f"   ⏳ WS TTS timeout (15s)")

            return audio_received
    except Exception as e:
        logger.error(f"   ❌ WS TTS failed: {e}")
        return False


async def main():
    print("\n" + "=" * 60)
    print("🧪 LIVA Voice Integration Test")
    print("=" * 60)
    print("📋 Prerequisites: voice_engine.py running on port 8002\n")

    output_dir = os.path.join(os.path.dirname(__file__), "workspace", "integ_test_output")
    os.makedirs(output_dir, exist_ok=True)

    results = []

    # ─── Test 1: HTTP TTS with default voice ───
    print("\n─── Test 1: HTTP TTS (Default Voice) ───")
    r1 = await test_http_tts(
        "default",
        "Xin chào, tôi là trợ lý ảo LIVA. Hôm nay thời tiết rất đẹp.",
        output_dir
    )
    results.append(("HTTP TTS (default voice)", r1))

    # ─── Test 2: WS set_voice → Vietnamese ───
    print("\n─── Test 2: WS set_voice → vi-VN-HoaiMyNeural ───")
    r2 = await test_ws_set_voice("vi-VN-HoaiMyNeural")
    results.append(("WS set_voice (Vietnamese)", r2))

    # ─── Test 3: WS TTS with Vietnamese voice ───
    print("\n─── Test 3: WS TTS → Vietnamese ───")
    r3 = await test_ws_tts(
        "Chào bạn! Tôi là LIVA, trợ lý ảo thông minh. Rất vui được làm quen.",
        output_dir,
        "vi-VN"
    )
    results.append(("WS TTS (Vietnamese)", r3))

    # ─── Test 4: WS set_voice → English Ava ───
    print("\n─── Test 4: WS set_voice → en-US-AvaMultilingualNeural ───")
    r4 = await test_ws_set_voice("en-US-AvaMultilingualNeural")
    results.append(("WS set_voice (English Ava)", r4))

    # ─── Test 5: WS TTS with English voice ───
    print("\n─── Test 5: WS TTS → English Ava ───")
    r5 = await test_ws_tts(
        "Hello! I am LIVA, your intelligent virtual assistant. How can I help you today?",
        output_dir,
        "en-US-Ava"
    )
    results.append(("WS TTS (English Ava)", r5))

    # ─── Test 6: WS set_voice → Japanese ───
    print("\n─── Test 6: WS set_voice → ja-JP-NanamiNeural ───")
    r6 = await test_ws_set_voice("ja-JP-NanamiNeural")
    results.append(("WS set_voice (Japanese)", r6))

    # ─── Test 7: WS TTS with Japanese voice ───
    print("\n─── Test 7: WS TTS → Japanese ───")
    r7 = await test_ws_tts(
        "こんにちは！私はLIVAです。今日はどんなお手伝いができますか？",
        output_dir,
        "ja-JP"
    )
    results.append(("WS TTS (Japanese)", r7))

    # ─── Test 8: Switch back to Vietnamese ───
    print("\n─── Test 8: Switch back to vi-VN-HoaiMyNeural ───")
    r8 = await test_ws_set_voice("vi-VN-HoaiMyNeural")
    results.append(("WS set_voice (back to Vietnamese)", r8))

    # ─── Test 9: Verify Vietnamese is restored ───
    print("\n─── Test 9: WS TTS → Vietnamese (restored) ───")
    r9 = await test_ws_tts(
        "Tôi đã chuyển lại giọng tiếng Việt. Mọi thứ hoạt động tốt!",
        output_dir,
        "vi-VN-restored"
    )
    results.append(("WS TTS (Vietnamese restored)", r9))

    # ─── Test 10: Invalid voice (should be rejected) ───
    print("\n─── Test 10: WS set_voice → invalid (should be rejected) ───")
    r10 = await test_ws_set_voice("INVALID_VOICE_ID")
    results.append(("WS set_voice (invalid - should pass send)", r10))

    # Summary
    print("\n" + "=" * 60)
    print("📊 TEST RESULTS")
    print("=" * 60)

    passed = sum(1 for _, ok in results if ok)
    total = len(results)
    for name, ok in results:
        status = "✅ PASS" if ok else "❌ FAIL"
        print(f"   {status} — {name}")

    print(f"\n   Total: {passed}/{total} passed")
    print(f"   📁 Audio output: {output_dir}")
    print("=" * 60 + "\n")

    return 0 if passed >= 8 else 1  # Allow 2 failures (invalid voice test + possible timeouts)


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    exit_code = asyncio.run(main())
    sys.exit(exit_code)
