# ==============================================================================
# LIVA SYSTEM - STARTUP GUIDE
# ==============================================================================
# Last Updated: 2026-05-14
# ==============================================================================

## 🚀 QUICK START (3 Steps)

### Step 1: Install Dependencies
```powershell
cd liva-ai-engine
.\venv\Scripts\pip install faster-whisper torch numpy
```

### Step 2: Add API Keys to .env
```powershell
# Edit liva-gateway/.env and add:
TAVILY_API_KEY=your_tavily_api_key_here
```

### Step 3: Start Everything
```powershell
npm run dev
```

---

## 📋 SYSTEM COMPONENTS

| Component | Port | Description |
|-----------|------|-------------|
| **LIVA Native Engine** | 8100 | gRPC LLM Inference (llama.cpp) |
| **Whisper STT Server** | 8101 | Speech-to-Text (faster-whisper) |
| **Voice Engine** | 8002 | TTS via Edge-TTS |
| **LIVA Gateway** | 8082 | WebSocket to UI |

---

## 🔧 MANUAL START (If auto script fails)

### Terminal 1: Whisper STT (Port 8101)
```powershell
cd liva-ai-engine
.\venv\Scripts\python.exe whisper_stt_server.py
```

### Terminal 2: LLM Engine (Port 8100)
```powershell
cd liva-ai-engine
.\venv\Scripts\python.exe liva_native_engine.py
```

### Terminal 3: Gateway
```powershell
cd liva-gateway
npm run dev
```

---

## ⚠️ TROUBLESHOOTING

### Issue: "ECONNREFUSED 127.0.0.1:8101"
**Cause**: Whisper STT server not running
**Fix**: Start whisper_stt_server.py

### Issue: "chưa rõ ý này" (AI doesn't know what to say)
**Cause**: 
1. No TAVILY_API_KEY configured
2. ProactiveDaemon hasn't fetched news yet
**Fix**: 
1. Add TAVILY_API_KEY to .env
2. Wait for morning briefing to be generated
3. Or: Restart Gateway to trigger briefing generation

### Issue: "Whisper Circuit OPEN"
**Cause**: Too many failed STT requests
**Fix**: Wait 15 seconds for auto-reset, then check Whisper server

### Issue: Voice doesn't work
**Cause**: Microphone permission not granted
**Fix**: Allow microphone access in browser/system

---

## 🎤 VOICE INTERACTION FLOW

```
User: "Hey Liva, trời hôm nay thế nào?"
  ↓
[Frontend] Mic detects wake word
  ↓
[Gateway] WhisperNode transcribes audio (port 8101)
  ↓
[SemanticRouter] Route: "news_briefing"
  ↓
[PromptBuilder] Inject daily_briefing from SQLite
  ↓
[Liva Native Engine] Generate response (port 8100)
  ↓
[VoiceEngine] Edge-TTS synthesis (port 8002)
  ↓
[Frontend] Play audio response
```

---

## 📡 CHECKING SERVICES

### Check if port is in use:
```powershell
netstat -an | Select-String "8100\|8101\|8082"
```

### Expected output:
```
TCP    127.0.0.1:8100    LISTENING    # LLM Engine
TCP    127.0.0.1:8101    LISTENING    # Whisper STT
TCP    127.0.0.1:8082    LISTENING    # Gateway WebSocket
```

---

## 🔐 ENVIRONMENT VARIABLES

| Variable | Required | Description |
|----------|----------|-------------|
| `AI_PROVIDER` | Yes | "local" or "cloud" |
| `TAVILY_API_KEY` | For news | Free at tavily.com |
| `WHISPER_URL` | For STT | Default: 8101 |
| `WHISPER_CLOUD_URL` | Optional | Cloud STT fallback |

