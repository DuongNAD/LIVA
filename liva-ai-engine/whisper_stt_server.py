"""
LIVA Whisper STT Server
=======================
FastAPI-based server for speech-to-text using faster-whisper.
Exposes OpenAI-compatible endpoint: POST /v1/audio/transcriptions

Usage:
    python whisper_stt_server.py

Ports:
    - Default: 8101 (configurable via WHISPER_PORT env var)
"""

import os
import sys
import io
import logging
import tempfile
import wave
from pathlib import Path
from typing import Optional

import uvicorn
from fastapi import FastAPI, UploadFile, File, Form, HTTPException
from fastapi.responses import JSONResponse, PlainTextResponse
import numpy as np

# Force UTF-8 on Windows
if sys.platform == "win32" and sys.stdout.encoding != "utf-8":
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8")

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='[Whisper] %(levelname)s: %(message)s'
)
logger = logging.getLogger("whisper")

app = FastAPI(title="LIVA Whisper STT")

# Configuration
PORT = int(os.getenv("WHISPER_PORT", "8101"))
MODEL_SIZE = os.getenv("WHISPER_MODEL", "base")

# Auto-detect CUDA availability - fallback to CPU if PyTorch CUDA incompatible
# RTX 50 series (Blackwell sm_120) not supported by PyTorch 2.5.x
def get_device():
    device = os.getenv("WHISPER_DEVICE", "auto")
    if device == "auto":
        try:
            import torch
            if torch.cuda.is_available():
                # Check if GPU is actually usable (not Blackwell with incompatible PyTorch)
                try:
                    torch.cuda.get_device_name(0)
                    return "cuda"
                except RuntimeError as e:
                    # CUDARuntimeError or similar — GPU incompatible with PyTorch version
                    logger.warning(f"GPU detected but incompatible with PyTorch ({e}) - using CPU")
                    return "cpu"
            return "cpu"
        except ImportError:
            return "cpu"
    return device

DEVICE = get_device()
COMPUTE_TYPE = "float16" if DEVICE == "cuda" else "int8"

# Global model instance
model: Optional[any] = None
model_loaded = False


def load_model():
    """Load faster-whisper model (lazy loading)."""
    global model, model_loaded
    if model_loaded:
        return model
        
    logger.info(f"Loading model: {MODEL_SIZE} on {DEVICE} ({COMPUTE_TYPE})...")

    try:
        from faster_whisper import WhisperModel

        # For faster-whisper >= 1.0, gpu_layers is not a valid parameter
        # Device selection is handled via device and device_index parameters
        model = WhisperModel(
            MODEL_SIZE,
            device=DEVICE if DEVICE != "cuda" else "cuda",
            device_index=int(os.getenv("CUDA_DEVICE_INDEX", "0")),
            compute_type=COMPUTE_TYPE,
        )
        model_loaded = True
        logger.info(f"Model loaded successfully!")
        return model

    except ImportError:
        logger.error("faster-whisper not installed. Run: pip install faster-whisper")
        return None


async def transcribe_audio(audio_bytes: bytes, language: Optional[str] = None) -> str:
    """Transcribe audio bytes to text."""
    global model, model_loaded

    # Lazy load model
    if not model_loaded:
        load_model()

    if model is None:
        return ""

    try:
        audio_array = None

        # Try WAV decode first
        if audio_bytes[:4] == b'RIFF':
            try:
                import wave as wave_module
                with wave_module.open(io.BytesIO(audio_bytes)) as wav:
                    frames = wav.readframes(wav.getnframes())
                    if wav.getsampwidth() == 2:
                        audio_array = np.frombuffer(frames, dtype=np.int16).astype(np.float32) / 32768.0
                    else:
                        audio_array = np.frombuffer(frames, dtype=np.float32)
            except wave_module.Error:
                # Not a valid WAV file — fall through to raw PCM
                pass

        # Fallback to raw PCM int16
        if audio_array is None:
            try:
                audio_array = np.frombuffer(audio_bytes, dtype=np.int16).astype(np.float32) / 32768.0
            except (ValueError, TypeError):
                # Cannot interpret bytes as int16 — try float32 directly
                try:
                    audio_array = np.frombuffer(audio_bytes, dtype=np.float32)
                except (ValueError, TypeError):
                    # Complete failure — cannot decode audio at all
                    logger.warning("Cannot decode audio bytes as WAV, int16 PCM, or float32 PCM")
                    return ""

        # Skip if audio too short (< 100ms)
        if len(audio_array) < 1600:
            return ""

        # Run transcription - optimized settings
        segments, info = model.transcribe(
            audio_array,
            language=language or "vi",
            beam_size=3,  # Reduced from 5 for faster inference
            vad_filter=True,
            vad_parameters=dict(min_silence_duration_ms=300),  # Reduced from 500ms
            # Performance optimizations
            best_of=2,  # Reduced from default
            patience=1.0,  # Reduced from 1.2
        )

        # Combine segments
        full_text = " ".join([segment.text for segment in segments])
        return full_text.strip()

    except Exception as e:
        logger.error(f"Transcription error: {e}")
        return ""


@app.get("/health")
async def health_check():
    """Health check endpoint."""
    return JSONResponse({
        "status": "ok" if model_loaded else "loading",
        "model": MODEL_SIZE if model_loaded else "not_loaded",
        "device": DEVICE
    })


@app.get("/")
async def root():
    """Root endpoint."""
    return JSONResponse({
        "service": "LIVA Whisper STT",
        "version": "1.0.0",
        "port": PORT,
        "model": MODEL_SIZE,
        "ready": model_loaded
    })


@app.post("/v1/audio/transcriptions")
async def transcribe_endpoint(
    file: UploadFile = File(...),
    model: str = Form(None),
    language: str = Form(None),
    response_format: str = Form("json"),
    prompt: str = Form(None),
    temperature: float = Form(0.0),
):
    """OpenAI-compatible transcription endpoint."""
    try:
        audio_content = await file.read()

        if len(audio_content) < 1000:  # Skip very small chunks
            if response_format == "text":
                return PlainTextResponse("")
            return JSONResponse({"text": ""})

        # Transcribe (log only on error or verbose mode)
        text = await transcribe_audio(audio_content, language)

        if response_format == "text":
            return PlainTextResponse(text)
        elif response_format == "srt":
            return PlainTextResponse(f"1\n00:00:00,000 --> 00:00:05,000\n{text}\n")
        elif response_format == "vtt":
            return PlainTextResponse(f"WEBVTT\n\n00:00:00.000 --> 00:00:05.000\n{text}\n")
        else:
            return JSONResponse({"text": text})

    except HTTPException:
        raise
    except Exception as e:
        logger.error(f"Error: {e}")
        raise HTTPException(status_code=500, detail=str(e))


def main():
    logger.info("=" * 60)
    logger.info("LIVA Whisper STT Server")
    logger.info(f"  Port: {PORT}")
    logger.info(f"  Model: {MODEL_SIZE}")
    logger.info(f"  Device: {DEVICE}")
    logger.info("=" * 60)

    # Check GPU
    if DEVICE == "cuda":
        try:
            import torch
            if torch.cuda.is_available():
                logger.info(f"  GPU: {torch.cuda.get_device_name(0)}")
                vram_gb = torch.cuda.get_device_properties(0).total_memory / 1e9
                logger.info(f"  VRAM: {vram_gb:.1f} GB")
            else:
                logger.warning("  CUDA not available, using CPU")
                os.environ["WHISPER_DEVICE"] = "cpu"
        except ImportError:
            logger.warning("  PyTorch not available, using CPU")
            os.environ["WHISPER_DEVICE"] = "cpu"

    # Pre-load model
    load_model()

    uvicorn.run(app, host="127.0.0.1", port=PORT, log_level="warning")


if __name__ == "__main__":
    main()
