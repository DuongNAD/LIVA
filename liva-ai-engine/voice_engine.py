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

# Sentence-splitting patterns using character classes instead of reluctant quantifiers (S5852)
_SENTENCE_END_PATTERN = re.compile(r'([^.?!]*[.?!])(\s+|$)')
_COMMA_SPLIT_PATTERN = re.compile(r'([^,]*,)(\s+|$)')


def _parse_sse_token(line: str):
    """Parse a single SSE line and return the content token, or None."""
    if not line.startswith("data:"):
        return None
    data_str = line[5:].strip()
    if data_str == "[DONE]" or not data_str:
        return None
    try:
        chunk = json.loads(data_str)
        delta = chunk["choices"][0].get("delta", {})
        return delta.get("content")
    except (json.JSONDecodeError, KeyError, IndexError):
        return None


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
                # [LEAK FIX] Ném Exception ngay nếu LLM server trả 4xx/5xx
                # thay vì im lặng đợi chunks từ response chết
                response.raise_for_status()
                async for line in response.aiter_lines():
                    if interrupt_event.is_set():
                        break
                    token = _parse_sse_token(line)
                    if token is not None:
                        yield token
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
            event_type = payload.get("type")

            if event_type == "interrupt":
                tts_worker_task, llm_generator_task = _handle_interrupt(tts_worker_task, llm_generator_task)
            elif event_type == "tts":
                await _handle_tts(payload, websocket)
            elif event_type == "prompt":
                tts_worker_task, llm_generator_task = await _handle_prompt_stream(
                    payload, websocket, tts_worker_task, llm_generator_task
                )

    except WebSocketDisconnect:
        print("🔴 [ Voice Engine 8002 ] Gateway đã ngắt kết nối.")
        await _cleanup_tasks(tts_worker_task, llm_generator_task)
        print("🧹 [ Voice Engine 8002 ] Đã dọn sạch asyncio tasks.")


def _handle_interrupt(tts_task, llm_task):
    """Cancel both running tasks on barge-in signal."""
    print("🛑 [ Voice Engine ] Nhận tín hiệu NGẮT LỜI. Hủy bỏ tác vụ sinh văn bản hiện hành...")
    if tts_task and not tts_task.done():
        tts_task.cancel()
    if llm_task and not llm_task.done():
        llm_task.cancel()
    return tts_task, llm_task


async def _handle_tts(payload: dict, websocket: WebSocket):
    """Synthesize text directly to audio (pure TTS mode)."""
    text = payload.get("text", "")
    print(f"🗣️ [ Voice Engine ] Đọc âm thanh (TTS): {text}")
    if text.strip():
        await synthesize_audio(text, websocket)


async def _handle_prompt_stream(
    payload: dict, websocket: WebSocket,
    prev_tts_task=None, prev_llm_task=None,
):
    """Run LLM + TTS pipeline for prompt-based conversation.
    
    [LEAK FIX] Cancel any still-running tasks from the previous prompt
    before creating new ones. Without this, rapid user prompts accumulate
    zombie tasks that consume CPU and memory indefinitely.
    """
    # Cancel previous tasks if they are still running
    if prev_tts_task and not prev_tts_task.done():
        prev_tts_task.cancel()
        try:
            await prev_tts_task
        except asyncio.CancelledError:  # NOSONAR - intentional
            pass
    if prev_llm_task and not prev_llm_task.done():
        prev_llm_task.cancel()
        try:
            await prev_llm_task
        except asyncio.CancelledError:  # NOSONAR - intentional
            pass

    messages = payload.get("messages", [])
    if not messages:
        text = payload.get("text", "")
        messages = [{"role": "user", "content": text}]

    print(f"🗣️ [ Voice Engine ] Xử lý luồng Chat ({len(messages)} câu thoại).")

    # Inject system prompt
    if not messages or messages[0].get("role") != "system":
        messages.insert(0, SYSTEM_PROMPT)
    else:
        messages[0] = SYSTEM_PROMPT

    interrupt_event = asyncio.Event()
    tts_queue: asyncio.Queue = asyncio.Queue()

    tts_worker_task = asyncio.create_task(_tts_worker(tts_queue, websocket))
    llm_generator_task = asyncio.create_task(
        _llm_runner(messages, interrupt_event, tts_queue, tts_worker_task, websocket)
    )
    return tts_worker_task, llm_generator_task


async def _tts_worker(tts_queue: asyncio.Queue, websocket: WebSocket):
    """Drain the TTS queue and synthesize each sentence."""
    while True:
        sentence = await tts_queue.get()
        if sentence is None:
            break
        await synthesize_audio(sentence, websocket)
        tts_queue.task_done()


async def _llm_runner(messages, interrupt_event: asyncio.Event, tts_queue: asyncio.Queue, tts_worker_task, websocket: WebSocket):
    """Stream LLM tokens, chunk into sentences, push to TTS queue."""
    sentence_buffer = ""
    try:
        async for token in llm_stream_generator(messages, interrupt_event):
            if interrupt_event.is_set():
                break
            if token:
                await websocket.send_text(json.dumps({"type": "text", "text": token}))
                sentence_buffer += token
                sentence_buffer = await _flush_sentences(sentence_buffer, tts_queue)

        # Drain remaining buffer
        if sentence_buffer.strip() and not interrupt_event.is_set():
            await tts_queue.put(sentence_buffer.strip())

        if not interrupt_event.is_set():
            await tts_queue.put(None)
            await tts_worker_task
            await websocket.send_text(json.dumps({"type": "turn_end"}))

    except asyncio.CancelledError:
        interrupt_event.set()
        raise  # Re-raise to let asyncio task scheduler handle properly


async def _flush_sentences(text_buf: str, tts_queue: asyncio.Queue) -> str:
    """Extract complete sentences from text_buf and push to TTS queue. Returns remainder."""
    while True:
        match = _SENTENCE_END_PATTERN.search(text_buf)
        if not match and len(text_buf) > 100:
            match = _COMMA_SPLIT_PATTERN.search(text_buf)
        if not match:
            break
        sentence = match.group(1).strip()
        if sentence:
            await tts_queue.put(sentence)
        text_buf = text_buf[len(match.group(0)):]
    return text_buf


async def _cleanup_tasks(tts_task, llm_task):
    """Cancel and await both tasks gracefully."""
    if tts_task and not tts_task.done():
        tts_task.cancel()
        try:
            await tts_task
        except asyncio.CancelledError:  # NOSONAR - intentional swallow after cancel() in cleanup
            pass
    if llm_task and not llm_task.done():
        llm_task.cancel()
        try:
            await llm_task
        except asyncio.CancelledError:  # NOSONAR - intentional swallow after cancel() in cleanup
            pass


if __name__ == "__main__":
    import sys
    sys.stdout.reconfigure(encoding='utf-8')
    print("==================================================")
    print("🎤 [LIVA VOICE] Khởi chạy Voice Engine Cục bộ (Cổng 8002)")
    print("==================================================")
    uvicorn.run(app, host="127.0.0.1", port=8002)
