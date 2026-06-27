# LIVA 2.0 - Voice Cloning Pipeline

> Clone giọng nói từ URL audio với chất lượng studio

## Features

- **VRAM Peak**: < 1.5GB (Sequential lazy loading)
- **Few-shot Learning**: Chỉ cần 1-5 phút audio
- **Vietnamese First**: Tối ưu cho tiếng Việt
- **Anti-Hallucination**: Lọc ảo giác Whisper
- **Speaker Verification**: Lọc giọng không phù hợp

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    LIVA 2.0 Pipeline                         │
├─────────────────────────────────────────────────────────────┤
│  1. Audio Prep    → DeepFilterNet3 + Silero VAD            │
│  2. Verify        → SpeechBrain ECAPA-TDNN                 │
│  3. STT           → Faster-Whisper (anti-hallucination)    │
│  4. Normalize     → VietnameseNormalizer (num2words)       │
│  5. Train         → GPT-SoVITS Core                        │
│  6. Validate      → Generate sample                         │
└─────────────────────────────────────────────────────────────┘
```

## Quick Start

```bash
# Install dependencies
pip install -r requirements.txt

# Clone a voice
python liva_voice.py --url "https://youtube.com/..." --name "my_voice"

# With reference audio
python liva_voice.py --url "..." --name "my_voice" --reference "ref.wav"
```

## API Usage

```python
from liva_voice import VoicePipeline

pipeline = VoicePipeline()
result = await pipeline.clone_voice(
    audio_url="https://youtube.com/...",
    voice_name="my_voice",
    reference_audio="reference.wav"  # Optional
)

print(result)
# {'status': 'success', 'model': 'models/my_voice.pth', 'sample': '...'}
```

## Requirements

- Python 3.10+
- CUDA-capable GPU (8GB VRAM recommended)
- ffmpeg

## License

MIT
