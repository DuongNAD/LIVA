import asyncio
import json
import base64
import re
import edge_tts
import httpx
from fastapi import FastAPI, WebSocket
from fastapi.websockets import WebSocketDisconnect
import uvicorn

app = FastAPI()
TTS_VOICE = "vi-VN-HoaiMyNeural"

SYSTEM_PROMPT = {
    "role": "system",
    "content": "Bạn là Liva, một trợ lý ảo bằng giọng nói. Hãy trả lời ngắn gọn, thân thiện, tự nhiên. TUYỆT ĐỐI KHÔNG sử dụng định dạng markdown (như in đậm, in nghiêng), ký tự đặc biệt hay biểu tượng cảm xúc. Hãy giao tiếp bằng tiếng Việt chuẩn."
}

async def llm_stream_generator(messages, interrupt_event: asyncio.Event):
    payload = {
        "model": "local-model",
        "messages": messages,
        "stream": True,
        "temperature": 0.5
    }
    # Tăng timeout để đợi mô hình suy nghĩ
    timeout = httpx.Timeout(120.0, connect=60.0)
    async with httpx.AsyncClient(timeout=timeout) as client:
        try:
            # Gửi HTTP Request sang cổng 8000 (OpenAI Compatible)
            async with client.stream("POST", "http://127.0.0.1:8000/v1/chat/completions", json=payload) as response:
                async for line in response.aiter_lines():
                    if interrupt_event.is_set():
                        # Đóng stream để dừng gọi GPU (Hủy connection)
                        break
                    
                    if line.startswith("data:"):
                        data_str = line[5:].strip()
                        if data_str == "[DONE]":
                            break
                        if not data_str:
                            continue
                        
                        try:
                            chunk = json.loads(data_str)
                            delta = chunk["choices"][0].get("delta", {})
                            if "content" in delta:
                                yield delta["content"]
                        except json.JSONDecodeError:
                            continue
        except Exception as e:
            print(f"Lỗi gọi LLM 8000: {e}")
            yield " Xin lỗi, hiện tại tôi không thể kết nối tới não bộ. "

async def synthesize_audio(text: str, websocket: WebSocket):
    if not text.strip(): return
    
    # Gọi thư viện tối ưu CPU Edge-TTS 
    communicate = edge_tts.Communicate(text, TTS_VOICE, rate="+15%")
    audio_data = bytearray()
    
    try:
        async for chunk in communicate.stream():
            if chunk["type"] == "audio":
                audio_data.extend(chunk["data"])
                
        if len(audio_data) > 0:
            b64_audio = base64.b64encode(audio_data).decode("utf-8")
            await websocket.send_text(json.dumps({
                "type": "audio",
                "data": b64_audio
            }))
    except Exception as e:
        print(f"Lỗi TTS: {e}")

@app.websocket("/ws")
async def voice_endpoint(websocket: WebSocket):
    await websocket.accept()
    print("🟢 [ Voice Engine 8002 ] Gateway đã kết nối.")
    
    tts_worker_task = None
    llm_generator_task = None

    try:
        while True:
            data = await websocket.receive_text()
            payload = json.loads(data)
            
            # Quản lý sự kiện Barge-in (Ngắt Lời) do người dùng ấn nút
            if payload.get("type") == "interrupt":
                print("🛑 [ Voice Engine ] Nhận tín hiệu NGẮT LỜI. Hủy bỏ tác vụ sinh văn bản hiện hành...")
                if tts_worker_task and not tts_worker_task.done():
                    tts_worker_task.cancel()
                if llm_generator_task and not llm_generator_task.done():
                    llm_generator_task.cancel()
                continue
            
            # Xử lý luồng TTS thuần túy (Node.js đã sinh ra logic)
            if payload.get("type") == "tts":
                text = payload.get("text", "")
                print(f"🗣️ [ Voice Engine ] Đọc âm thanh (TTS): {text}")
                if text.strip():
                    await synthesize_audio(text, websocket)
                continue
            
            # Quản lý sự kiện Trả lời (LLM cục bộ - Hiện Node.js đã gánh phần này nên ít xài)
            if payload.get("type") == "prompt":
                messages = payload.get("messages", [])
                if not messages:
                    text = payload.get("text", "")
                    messages = [{"role": "user", "content": text}]
                
                print(f"🗣️ [ Voice Engine ] Xử lý luồng Chat ({len(messages)} câu thoại).")
                
                # Tiêm Spoken-friendly System Prompt
                if len(messages) == 0 or messages[0].get("role") != "system":
                    messages.insert(0, SYSTEM_PROMPT)
                else:
                    messages[0] = SYSTEM_PROMPT
                    
                interrupt_event = asyncio.Event()
                tts_queue = asyncio.Queue()
                
                async def tts_worker():
                    try:
                        while True:
                            sentence = await tts_queue.get()
                            if sentence is None:
                                break
                            await synthesize_audio(sentence, websocket)
                            tts_queue.task_done()
                    except asyncio.CancelledError:
                        raise  # Re-raise để asyncio task scheduler xử lý đúng
                        
                tts_worker_task = asyncio.create_task(tts_worker())
                sentence_buffer = ""
                
                async def llm_runner():
                    nonlocal sentence_buffer
                    try:
                        async for token in llm_stream_generator(messages, interrupt_event):
                            if interrupt_event.is_set():
                                break
                            
                            if token:
                                # Stream real-time text cho tính năng gõ màn hình
                                await websocket.send_text(json.dumps({"type": "text", "text": token}))
                                sentence_buffer += token
                                
                                # Chunking Regex (Ưu tiên chấm câu rõ ràng)
                                while True:
                                    match = re.search(r'(.*?[.?!])(\s+|$)', sentence_buffer)
                                    if not match:
                                        # Nếu câu vượt quá 100 ký tự mà vẫn chưa hết câu, xé tạm bằng dấu phẩy
                                        if len(sentence_buffer) > 100:
                                           match_comma = re.search(r'(.*?,)(\s+|$)', sentence_buffer)
                                           if match_comma:
                                               match = match_comma
                                    
                                    if not match:
                                        break
                                    
                                    sentence = match.group(1)
                                    if sentence.strip():
                                        await tts_queue.put(sentence.strip())
                                        
                                    sentence_buffer = sentence_buffer[len(match.group(0)):]
                                    
                        # Quét dọn bộ đệm
                        if sentence_buffer.strip() and not interrupt_event.is_set():
                            await tts_queue.put(sentence_buffer.strip())
                            
                        # Gửi tín hiệu đóng worker
                        if not interrupt_event.is_set():
                            await tts_queue.put(None)
                            await tts_worker_task
                            await websocket.send_text(json.dumps({"type": "turn_end"}))
                            
                    except asyncio.CancelledError:
                        interrupt_event.set()  # Bắn cờ đóng Http Stream kết nối 8000
                        raise  # Re-raise để asyncio task scheduler xử lý đúng

                llm_generator_task = asyncio.create_task(llm_runner())
                
    except WebSocketDisconnect:
        print("🔴 [ Voice Engine 8002 ] Gateway đã ngắt kết nối.")
        # 🔒 [Memory Fix #8] Hủy bỏ các asyncio Task đang treo để tránh zombie tasks
        if tts_worker_task and not tts_worker_task.done():
            tts_worker_task.cancel()
            try:
                await tts_worker_task
            except asyncio.CancelledError:  # NOSONAR - intentional swallow after cancel() in cleanup
                pass
        if llm_generator_task and not llm_generator_task.done():
            llm_generator_task.cancel()
            try:
                await llm_generator_task
            except asyncio.CancelledError:  # NOSONAR - intentional swallow after cancel() in cleanup
                pass
        print("🧹 [ Voice Engine 8002 ] Đã dọn sạch asyncio tasks.")

if __name__ == "__main__":
    import sys
    sys.stdout.reconfigure(encoding='utf-8')
    print("==================================================")
    print("🎤 [LIVA VOICE] Khởi chạy Voice Engine Cục bộ (Cổng 8002)")
    print("==================================================")
    uvicorn.run(app, host="127.0.0.1", port=8002)
