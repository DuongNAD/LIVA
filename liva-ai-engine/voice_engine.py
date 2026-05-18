import asyncio
import json
import base64
import re
import os
import logging
import edge_tts
import httpx
from fastapi import FastAPI, WebSocket, HTTPException
from fastapi.websockets import WebSocketDisconnect
import uvicorn

logging.basicConfig(
    level=logging.INFO,
    format="[Voice] %(levelname)s: %(message)s",
)
logger = logging.getLogger("voice_engine")
from pydantic import BaseModel

class TTSRequest(BaseModel):
    text: str

app = FastAPI()
# [v25] Global mutable voice — can be changed at runtime via WS 'set_voice' event
TTS_VOICE = os.getenv("LIVA_TTS_VOICE", "vi-VN-HoaiMyNeural")

# Whitelist of allowed Edge-TTS voices to prevent injection
ALLOWED_VOICES = {
    "vi-VN-HoaiMyNeural",
    "vi-VN-NamMinhNeural",
    "en-US-AvaMultilingualNeural",
    "en-US-AriaNeural",
    "en-US-JennyNeural",
    "en-US-MichelleNeural",
    "en-US-EmmaMultilingualNeural",
    "en-US-EmmaNeural",
    "en-US-AnaNeural",
    "ja-JP-NanamiNeural",
    "ko-KR-SunHiNeural",
    "zh-CN-XiaoxiaoNeural",
    "zh-CN-XiaoyiNeural",
}

# ═══════════════════════════════════════════════════════
#  [P5] TTS Text Sanitizer — Defense-in-depth
#  Gateway TTSFormatter strips most artifacts, but this
#  is a last-resort filter in case raw text leaks through.
# ═══════════════════════════════════════════════════════
_EMOJI_PATTERN = re.compile(
    "["
    "\U0001F600-\U0001F64F"  # emoticons
    "\U0001F300-\U0001F5FF"  # symbols & pictographs
    "\U0001F680-\U0001F6FF"  # transport & map
    "\U0001F1E0-\U0001F1FF"  # flags
    "\U0001F900-\U0001F9FF"  # supplemental
    "\U0001FA00-\U0001FA6F"  # chess
    "\U0001FA70-\U0001FAFF"  # symbols extended
    "\U00002600-\U000026FF"  # misc symbols
    "\U00002700-\U000027BF"  # dingbats
    "\U0000200D"             # zero width joiner
    "\U0000FE0F"             # variation selector
    "]+", flags=re.UNICODE
)
_CODE_BLOCK = re.compile(r'```[\s\S]*?```')
_INLINE_CODE = re.compile(r'`[^`]+`')
_URL = re.compile(r'https?://[^\s)>\]]+', re.IGNORECASE)
_MARKDOWN_BOLD = re.compile(r'\*{1,3}([^*]+)\*{1,3}')
_MARKDOWN_UNDER = re.compile(r'_{1,2}([^_]+)_{1,2}')
_MARKDOWN_HEADER = re.compile(r'^#{1,6}\s*', re.MULTILINE)
_ANGLE_BRACKETS = re.compile(r'[<>]')
_MULTI_SPACE = re.compile(r'\s{2,}')

def sanitize_for_tts(text: str) -> str:
    """Strip non-speakable artifacts from text before TTS synthesis."""
    result = _CODE_BLOCK.sub('', text)
    result = _INLINE_CODE.sub('', result)
    result = _URL.sub('', result)
    result = _MARKDOWN_BOLD.sub(r'\1', result)
    result = _MARKDOWN_UNDER.sub(r'\1', result)
    result = _MARKDOWN_HEADER.sub('', result)
    result = _ANGLE_BRACKETS.sub(' ', result)
    result = _EMOJI_PATTERN.sub('', result)
    result = _MULTI_SPACE.sub(' ', result)
    return result.strip()

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
            logger.info(f"Lỗi gọi LLM 8000: {e}")
            yield " Xin lỗi, hiện tại tôi không thể kết nối tới não bộ. "

async def synthesize_audio(text: str, websocket: WebSocket, max_retries=2, voice_override: str | None = None):
    text = sanitize_for_tts(text)
    if not text.strip(): return
    voice_to_use = voice_override or TTS_VOICE
    
    for attempt in range(max_retries + 1):
        audio_data = bytearray()
        try:
            # Gọi thư viện tối ưu CPU Edge-TTS 
            communicate = edge_tts.Communicate(text, voice_to_use, rate="+15%")
            async for chunk in communicate.stream():
                if chunk["type"] == "audio":
                    audio_data.extend(chunk["data"])
                    
            if len(audio_data) > 0:
                b64_audio = base64.b64encode(audio_data).decode("utf-8")
                await websocket.send_text(json.dumps({
                    "type": "audio",
                    "data": b64_audio
                }))
                return # Thành công, thoát vòng lặp
        except Exception as e:
            if attempt < max_retries:
                logger.info(f"⚠️ [Voice Engine] Lỗi TTS ngắt kết nối Azure (thử lại {attempt + 1}/{max_retries})...")
                await asyncio.sleep(0.5)
            else:
                logger.info(f"Lỗi TTS: {e}")

@app.post("/tts")
async def tts_endpoint(req: TTSRequest):
    clean_text = sanitize_for_tts(req.text)
    if not clean_text.strip():
        return {"status": "empty"}
    
    communicate = edge_tts.Communicate(clean_text, TTS_VOICE, rate="+15%")
    audio_data = bytearray()
    try:
        async for chunk in communicate.stream():
            if chunk["type"] == "audio":
                audio_data.extend(chunk["data"])
        
        if len(audio_data) > 0:
            b64_audio = base64.b64encode(audio_data).decode("utf-8")
            return {"status": "ok", "audio": b64_audio}
        return {"status": "empty"}
    except Exception as e:
        logger.info(f"Lỗi TTS HTTP: {e}")
        raise HTTPException(status_code=500, detail=str(e))

@app.websocket("/ws")
async def voice_endpoint(websocket: WebSocket):
    await websocket.accept()
    logger.info("🟢 [ Voice Engine 8002 ] Gateway đã kết nối.")

    tts_worker_task = None
    llm_generator_task = None

    try:
        while True:
            try:
                data = await asyncio.wait_for(websocket.receive_text(), timeout=60.0)
            except asyncio.TimeoutError:
                logger.warning("[ Voice Engine 8002 ] No message in 60s, closing connection.")
                break

            # Parse message — fail fast on malformed JSON instead of crashing
            try:
                payload = json.loads(data)
            except (json.JSONDecodeError, ValueError, TypeError) as e:
                logger.warning(f"[ Voice Engine 8002 ] Malformed JSON message: {e}")
                continue

            event_type = payload.get("type")

            if event_type == "interrupt":
                tts_worker_task, llm_generator_task = _handle_interrupt(tts_worker_task, llm_generator_task)
            elif event_type == "set_voice":
                _handle_set_voice(payload)
            elif event_type == "tts":
                await _handle_tts(payload, websocket)
            elif event_type == "prompt":
                tts_worker_task, llm_generator_task = await _handle_prompt_stream(
                    payload, websocket, tts_worker_task, llm_generator_task
                )

    except WebSocketDisconnect:
        logger.info("🔴 [ Voice Engine 8002 ] Gateway đã ngắt kết nối.")
    except Exception as e:
        logger.error(f"[ Voice Engine 8002 ] Unexpected error: {e}")
    finally:
        await _cleanup_tasks(tts_worker_task, llm_generator_task)
        logger.info("🧹 [ Voice Engine 8002 ] Đã dọn sạch asyncio tasks.")


def _handle_interrupt(tts_task, llm_task):
    """Cancel both running tasks on barge-in signal."""
    logger.info("🛑 [ Voice Engine ] Nhận tín hiệu NGẮT LỜI. Hủy bỏ tác vụ sinh văn bản hiện hành...")
    if tts_task and not tts_task.done():
        tts_task.cancel()
    if llm_task and not llm_task.done():
        llm_task.cancel()
    return tts_task, llm_task


def _handle_set_voice(payload: dict):
    """Switch Edge-TTS voice at runtime."""
    global TTS_VOICE
    new_voice = payload.get("voice", "").strip()
    if not new_voice:
        logger.warning("[ Voice Engine ] set_voice: empty voice ID, ignoring.")
        return
    if new_voice not in ALLOWED_VOICES:
        logger.warning(f"[ Voice Engine ] set_voice: '{new_voice}' not in whitelist, ignoring.")
        return
    old_voice = TTS_VOICE
    TTS_VOICE = new_voice
    logger.info(f"🎤 [ Voice Engine ] Voice changed: {old_voice} → {new_voice}")


async def _handle_tts(payload: dict, websocket: WebSocket):
    """Synthesize text directly to audio (pure TTS mode)."""
    text = payload.get("text", "")
    logger.info(f"🗣️ [ Voice Engine ] Đọc âm thanh (TTS): {text}")
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

    logger.info(f"🗣️ [ Voice Engine ] Xử lý luồng Chat ({len(messages)} câu thoại).")

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
    if hasattr(sys.stdout, "reconfigure"):
        getattr(sys.stdout, "reconfigure")(encoding="utf-8")
    logger.info("==================================================")
    logger.info("🎤 [LIVA VOICE] Khởi chạy Voice Engine Cục bộ (Cổng 8002)")
    logger.info("==================================================")
    uvicorn.run(app, host="127.0.0.1", port=8002)
