"""
FastAPI Integration cho LIVA Voice Cloning

Exposes voice cloning as REST API endpoint.
"""

import asyncio
import tempfile
from pathlib import Path
from typing import Optional

from fastapi import FastAPI, HTTPException, BackgroundTasks
from fastapi.responses import FileResponse
from pydantic import BaseModel, Field
import uvicorn

from src.voice_pipeline import VoicePipeline, CloneResult


# Models
class CloneRequest(BaseModel):
    """Request model cho voice cloning"""
    audio_url: str = Field(..., description="Audio URL (YouTube, direct link)")
    voice_name: str = Field(..., description="Tên giọng (unique identifier)")
    reference_audio: Optional[str] = Field(None, description="Reference audio path")
    do_speaker_verify: bool = Field(True, description="Có verify speaker không")


class CloneResponse(BaseModel):
    """Response model"""
    status: str
    message: str
    task_id: Optional[str] = None


class TaskStatus(BaseModel):
    """Task status model"""
    task_id: str
    status: str  # "pending", "running", "completed", "failed"
    result: Optional[dict] = None
    error: Optional[str] = None


# FastAPI App
app = FastAPI(
    title="LIVA 2.0 Voice Cloning API",
    description="Voice cloning pipeline for LIVA",
    version="2.0.0",
)

# Global state
tasks = {}
pipeline = VoicePipeline()


# Routes
@app.get("/")
async def root():
    """Health check"""
    return {
        "service": "LIVA 2.0 Voice Cloning",
        "version": "2.0.0",
        "status": "ready"
    }


@app.get("/health")
async def health():
    """Health check"""
    from src.vram_manager import VRAMManager
    
    status = {
        "service": "healthy",
        "gpu_available": VRAMManager.is_cuda_available,
    }
    
    if VRAMManager.is_cuda_available:
        status["vram_free_mb"] = VRAMManager.get_free_vram_mb()
        status["vram_total_mb"] = VRAMManager.get_total_vram_mb()
    
    return status


@app.post("/clone", response_model=CloneResponse)
async def clone_voice(
    request: CloneRequest,
    background_tasks: BackgroundTasks
):
    """
    Clone giọng từ URL audio
    
    Request:
    ```json
    {
        "audio_url": "https://youtube.com/...",
        "voice_name": "my_voice",
        "reference_audio": "/path/to/reference.wav",  // optional
        "do_speaker_verify": true
    }
    ```
    
    Response:
    ```json
    {
        "status": "success",
        "message": "Clone started",
        "task_id": "abc123"
    }
    ```
    """
    import uuid
    
    # Generate task ID
    task_id = str(uuid.uuid4())[:8]
    
    # Initialize task
    tasks[task_id] = {
        "status": "pending",
        "result": None,
        "error": None
    }
    
    # Run in background
    background_tasks.add_task(
        run_clone_task,
        task_id,
        request.audio_url,
        request.voice_name,
        request.reference_audio,
        request.do_speaker_verify
    )
    
    return CloneResponse(
        status="success",
        message=f"Clone task started: {task_id}",
        task_id=task_id
    )


@app.get("/status/{task_id}", response_model=TaskStatus)
async def get_status(task_id: str):
    """Get task status"""
    if task_id not in tasks:
        raise HTTPException(status_code=404, detail="Task not found")
    
    task = tasks[task_id]
    return TaskStatus(
        task_id=task_id,
        status=task["status"],
        result=task.get("result"),
        error=task.get("error")
    )


@app.get("/result/{task_id}")
async def get_result(task_id: str):
    """Get task result (model and sample paths)"""
    if task_id not in tasks:
        raise HTTPException(status_code=404, detail="Task not found")
    
    task = tasks[task_id]
    
    if task["status"] != "completed":
        raise HTTPException(
            status_code=400,
            detail=f"Task not completed: {task['status']}"
        )
    
    result = task["result"]
    
    return {
        "model_path": result.get("model_path"),
        "sample_path": result.get("sample_path"),
        "stats": result.get("stats"),
    }


@app.get("/voices")
async def list_voices():
    """List all available voices"""
    voices = pipeline.list_voices()
    return {"voices": voices}


@app.delete("/voices/{voice_name}")
async def delete_voice(voice_name: str):
    """Delete a voice model"""
    model_path = pipeline.workspace / "models" / voice_name
    
    if not model_path.exists():
        raise HTTPException(status_code=404, detail="Voice not found")
    
    import shutil
    shutil.rmtree(model_path)
    
    return {"status": "deleted", "voice_name": voice_name}


@app.post("/inference")
async def run_inference(
    voice_name: str,
    text: str,
    reference_audio: Optional[str] = None
):
    """
    Run inference với trained voice model
    
    Request:
    ```
    voice_name: my_voice
    text: "Xin chào, tôi là LIVA"
    reference_audio: /path/to/audio.wav  // optional
    ```
    """
    from src.gpt_sovits_core import GPTSoVITSCore
    from src.vram_manager import VRAMManager
    from src.vietnamese_normalizer import get_normalizer
    
    # Get model path
    model_path = pipeline.workspace / "models" / voice_name
    
    if not model_path.exists():
        raise HTTPException(status_code=404, detail="Voice not found")
    
    # Normalize text
    normalizer = get_normalizer()
    text_normalized = normalizer.normalize(text)
    
    # Generate output path
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
        output_path = Path(f.name)
    
    # Run inference
    gpt_sovits = GPTSoVITSCore()
    
    try:
        # Get reference audio
        if reference_audio is None:
            chunks_dir = pipeline.workspace / "chunks"
            chunks = list(chunks_dir.glob("*.wav"))
            if chunks:
                reference_audio = str(chunks[0])
        
        async with VRAMManager.gpu_lock:
            result_path = await gpt_sovits.inference(
                model_dir=model_path,
                text=text_normalized,
                reference_audio=Path(reference_audio) if reference_audio else None,
                output_path=output_path,
            )
        
        return FileResponse(
            path=result_path,
            filename=f"{voice_name}_output.wav",
            media_type="audio/wav"
        )
        
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


# Background task
async def run_clone_task(
    task_id: str,
    audio_url: str,
    voice_name: str,
    reference_audio: Optional[str],
    do_speaker_verify: bool
):
    """Run clone task in background"""
    tasks[task_id]["status"] = "running"
    
    try:
        result = await pipeline.clone_voice(
            audio_url=audio_url,
            voice_name=voice_name,
            reference_audio=reference_audio,
            do_speaker_verify=do_speaker_verify,
        )
        
        if result.status == "success":
            tasks[task_id]["status"] = "completed"
            tasks[task_id]["result"] = {
                "model_path": result.model_path,
                "sample_path": result.sample_path,
                "stats": result.stats,
            }
        else:
            tasks[task_id]["status"] = "failed"
            tasks[task_id]["error"] = result.error
            
    except Exception as e:
        tasks[task_id]["status"] = "failed"
        tasks[task_id]["error"] = str(e)


# CLI to run server
def run_server(host: str = "0.0.0.0", port: int = 8765):
    """Run FastAPI server"""
    uvicorn.run(
        "liva_api:app",
        host=host,
        port=port,
        reload=False,
    )


if __name__ == "__main__":
    import argparse
    
    parser = argparse.ArgumentParser(description="LIVA 2.0 API Server")
    parser.add_argument("--host", default="0.0.0.0", help="Host")
    parser.add_argument("--port", type=int, default=8765, help="Port")
    
    args = parser.parse_args()
    
    print(f"Starting LIVA 2.0 API Server on {args.host}:{args.port}")
    run_server(args.host, args.port)
