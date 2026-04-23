# 🏗️ LIVA — Codebase Architecture Diagram

> Mở file này trong VS Code, chuột phải → **"Preview Mermaid"** để xem sơ đồ trực quan.

---

## 1. Tổng Quan Kiến Trúc Hệ Thống (System Overview)

```mermaid
graph TD
    User([👤 Người dùng])

    subgraph UI ["🖥️ liva-ui (Vue 3 + Electron)"]
        AppVue["App.vue"]
        VoiceChat["VoiceChat.vue"]
        Live2D["Live2D Avatar (PIXI.js)"]
        AudioPlayer["Web Audio API"]
    end

    subgraph GW ["⚙️ openclaw-gateway (Node.js TypeScript)"]
        Gateway["Gateway.ts (Entry Point)"]
        
        subgraph Core ["🧠 Core"]
            CoreKernel["CoreKernel"]
            AgentLoop["AgentLoop"]
            ModelOrchestrator["ModelOrchestrator"]
            PromptBuilder["PromptBuilder"]
            UIController["UIController (WebSocket:8082)"]
            ZaloPolling["ZaloPolling"]
            TelemetryProfiler["TelemetryProfiler"]
        end

        subgraph SubAgents ["🤖 Sub-Agents (trong AgentLoop)"]
            DualPort["DualPortController"]
            ToolOrch["ToolExecutionOrchestrator"]
            LTCOrch["LTCOrchestrator"]
            TaskLaneWorker["TaskLaneWorker (Pub/Sub)"]
        end

        subgraph Memory ["💾 Memory"]
            MemoryManager["MemoryManager"]
            TurboQuantStore["TurboQuantStore"]
            SensoryManager["SensoryManager"]
            HeraCompass["HeraCompass"]
            LanceMemory["LanceMemory"]
        end

        subgraph Skills ["🔧 Skills (27 plugins)"]
            WebSearch["WebSearch"]
            WebBrowser["WebBrowser"]
            SendZaloRPA["SendZaloRPA"]
            SendZaloBot["SendZaloBot"]
            ReadEmails["ReadEmails"]
            ExecuteCommand["ExecuteCommand"]
            GitSync["GitSyncProject"]
            AIScientist["AIScientist"]
            PlanWriter["PlanWriter"]
            ReportWriter["ReportWriter"]
            FileOps["Read/Write/Delete Files"]
            GoogleDocs["Google Docs/Sheets"]
            OtherSkills["...và nhiều skill khác"]
        end

        subgraph Security ["🔒 Security"]
            ZMASGuard["ZMAS_Guard"]
        end

        subgraph Evolution ["🧬 Evolution"]
            DarwinianEvolver["DarwinianEvolver"]
            LearningLog["LearningLog"]
            QualityChecker["QualityChecker"]
            WebResearchAgent["WebResearchAgent"]
            StructuredExtractor["StructuredExtractor"]
        end

        subgraph Infra ["🏭 Infrastructure"]
            SkillRegistry["SkillRegistry"]
            NativeIPCClient["NativeIPCClient (TCP:8100)"]
            ShieldGuard["ShieldGuard"]
            DockerSandbox["DockerSandbox"]
            BlueGreenRouter["BlueGreenRouter"]
            VoiceRelay["VoiceRelay"]
            PluginSDK["PluginSDK"]
        end

        subgraph Services ["🎙️ Services"]
            VoiceEngine["VoiceEngine (edge-tts)"]
            WhisperNode["WhisperNode (STT)"]
        end

        AutoSingularity["auto_singularity.ts (Tự Tiến Hóa)"]
    end

    subgraph Engine ["🐍 liva-ai-engine (Python)"]
        EnginePy["engine.py (llama-cpp-python/Uvicorn)"]
        NativeEngine["liva_native_engine.py (ctypes CFFI)"]
        VoiceEnginePy["voice_engine.py (edge-tts)"]
        LlamaDLL["llama.dll (CUDA GPU)"]
    end

    subgraph Models ["🧊 AI Models (E:/AI_Models)"]
        RouterModel["Gemma 4 E4B (Router 4B)"]
        ExpertModel["Gemma 4 26B (Expert)"]
    end

    %% === Luồng chính ===
    User -->|"Gõ/Nói"| AppVue
    AppVue -->|"WebSocket :8082"| UIController
    UIController -->|"emit user_input"| CoreKernel
    CoreKernel -->|"dispatch agent_input"| AgentLoop

    %% === Zalo Input ===
    User -->|"Nhắn Zalo"| ZaloPolling
    ZaloPolling -->|"emit zalo_incoming"| CoreKernel

    %% === AgentLoop xử lý ===
    AgentLoop --> PromptBuilder
    AgentLoop --> DualPort
    AgentLoop --> ToolOrch
    AgentLoop --> LTCOrch
    AgentLoop --> TaskLaneWorker

    PromptBuilder --> MemoryManager
    PromptBuilder --> SensoryManager

    ToolOrch --> SkillRegistry
    ToolOrch --> ZMASGuard
    LTCOrch --> MemoryManager

    %% === AI Inference ===
    AgentLoop -->|"OpenAI SDK / HTTP :8000"| EnginePy
    AgentLoop -->|"JSONL/TCP :8100"| NativeIPCClient
    NativeIPCClient -->|"TCP Socket"| NativeEngine
    NativeEngine -->|"ctypes CFFI"| LlamaDLL
    EnginePy --> RouterModel
    DualPort --> ModelOrchestrator
    ModelOrchestrator -->|"spawn llama-server :8001"| ExpertModel
    ModelOrchestrator -->|"Health Check :8000"| EnginePy

    %% === Memory ===
    MemoryManager --> TurboQuantStore
    TurboQuantStore -->|"Xenova Embedding"| MemoryManager

    %% === Skills ===
    SkillRegistry --> WebSearch
    SkillRegistry --> WebBrowser
    SkillRegistry --> SendZaloRPA
    SkillRegistry --> ExecuteCommand
    SkillRegistry --> AIScientist
    SkillRegistry --> FileOps

    %% === Voice Pipeline ===
    AppVue -->|"Binary Audio"| UIController
    UIController -->|"emit audio_input"| WhisperNode
    WhisperNode -->|"transcription_ready"| CoreKernel
    AgentLoop -->|"onStreamChunk"| VoiceEngine
    VoiceEngine -->|"audio_base64"| UIController
    UIController -->|"WebSocket"| AudioPlayer

    %% === Output ===
    AgentLoop -->|"onStreamChunk"| CoreKernel
    CoreKernel -->|"broadcastUIEvent"| UIController
    UIController -->|"WebSocket"| AppVue
    AppVue --> Live2D

    %% === Singularity ===
    AutoSingularity -->|"Hot-Swap VRAM"| ModelOrchestrator
    AutoSingularity --> SkillRegistry
    AutoSingularity --> LanceMemory
    AutoSingularity --> DarwinianEvolver

    %% === Styling ===
    classDef ui fill:#1a1a2e,stroke:#e94560,stroke-width:2px,color:#fff
    classDef core fill:#0f3460,stroke:#16213e,stroke-width:2px,color:#fff
    classDef memory fill:#533483,stroke:#e94560,stroke-width:2px,color:#fff
    classDef skill fill:#1a535c,stroke:#4ecdc4,stroke-width:2px,color:#fff
    classDef engine fill:#2d132c,stroke:#ee4540,stroke-width:2px,color:#fff
    classDef model fill:#0d7377,stroke:#14ffec,stroke-width:2px,color:#fff
    classDef security fill:#6a0572,stroke:#ab4e68,stroke-width:2px,color:#fff

    class AppVue,VoiceChat,Live2D,AudioPlayer ui
    class Gateway,CoreKernel,AgentLoop,ModelOrchestrator,PromptBuilder,UIController,ZaloPolling,TelemetryProfiler core
    class MemoryManager,TurboQuantStore,SensoryManager,HeraCompass,LanceMemory memory
    class WebSearch,WebBrowser,SendZaloRPA,SendZaloBot,ReadEmails,ExecuteCommand,GitSync,AIScientist,PlanWriter,ReportWriter,FileOps,GoogleDocs,OtherSkills skill
    class EnginePy,NativeEngine,VoiceEnginePy,LlamaDLL engine
    class RouterModel,ExpertModel model
    class ZMASGuard,ShieldGuard security
```

---

## 2. Luồng Xử Lý Tin Nhắn (Message Flow)

```mermaid
sequenceDiagram
    actor User as 👤 Người dùng
    participant UI as Liva UI (Vue)
    participant WS as UIController (WS:8082)
    participant CK as CoreKernel
    participant AL as AgentLoop
    participant PB as PromptBuilder
    participant MM as MemoryManager
    participant AI as AI Engine (Router 4B)
    participant SK as SkillRegistry
    participant TO as ToolOrchestrator

    User->>UI: Gõ tin nhắn
    UI->>WS: WebSocket JSON {user_voice_command}
    WS->>CK: emit("user_input", text)
    CK->>CK: dispatch("agent_input") via CommandToken
    CK->>AL: handleUserInput(text)
    
    Note over AL: TaskLaneWorker nhận task<br/>trên lane LLM_REASONING

    AL->>PB: prepareFullAiMessages()
    PB->>MM: getHybridContext() (RAG)
    MM-->>PB: [Ký ức cũ + Lịch sử gần]
    PB->>PB: buildToolsPrompt() (Semantic Filter)
    PB-->>AL: Full AI Messages array

    AL->>AI: chat.completions.create({stream: true})
    
    loop Streaming tokens
        AI-->>AL: token chunk
        AL->>WS: onStreamChunk → broadcastUIEvent
        WS-->>UI: {ai_stream_chunk}
        UI->>UI: Cập nhật bubble + Live2D animation
    end

    alt AI gọi Tool (XML <tool_call>)
        AL->>AL: Parse XML → toolCalls[]
        AL->>TO: executeWithReflection(toolName, args)
        TO->>SK: executeSkill()
        SK-->>TO: Kết quả
        TO->>TO: ZMAS_Guard.autoRemediation()
        TO->>TO: Reflection Layer (heuristic)
        TO-->>AL: {resultStr, valid, rawObj}
        
        Note over AL: Nạp kết quả tool vào<br/>context → Lặp lại AI call
    end

    alt AI yêu cầu Handoff Expert
        AL->>AL: DualPortController.ensureExpertReady()
        AL->>AI: stopRouter()
        AL->>AI: startExpert(:8001, 26B model)
        Note over AI: Chuyển sang Expert 26B
        AL->>AI: Gọi lại với Expert client
    end

    AL->>MM: addMessage("user", text)
    AL->>MM: addMessage("assistant", reply)
    AL->>AL: LTCOrchestrator.summarizeAndStore()
    AL->>WS: onSpokenResponse(finalReply)
    WS-->>UI: {ai_spoken_response}
```

---

## 3. Kiến Trúc Bộ Nhớ (Memory Architecture)

```mermaid
graph LR
    subgraph Input
        UserMsg["User Message"]
        AIReply["AI Reply"]
    end

    subgraph MemoryManager ["MemoryManager"]
        AddMsg["addMessage()"]
        HybridCtx["getHybridContext()"]
        LTM["getLongTermContext()"]
        Profile["getUserProfile()"]
    end

    subgraph QuantStore ["TurboQuantStore"]
        direction TB
        CoreKernelAuth["CoreKernel (Authority)"]
        TensorEngine["SelfHealingTensorStore"]
        RoleIndex["Role-based Map Index"]
        ECC["ECC Residual (Self-Healing)"]
        GC["Background GC (5min)"]
    end

    subgraph Xenova ["Xenova Embeddings"]
        MiniLM["all-MiniLM-L6-v2"]
    end

    subgraph Storage ["Disk Storage"]
        JSONL["turbo_quant_memory.jsonl"]
        EncFile["long_term_memory.enc (AES-256-GCM)"]
        ProfileJSON["user_profile.json"]
    end

    subgraph Sensory ["SensoryManager (Singleton)"]
        ActiveWin["active-win"]
        Clipboard["clipboardy"]
        TTL["TTL: 30s auto-expire"]
    end

    UserMsg --> AddMsg
    AIReply --> AddMsg
    AddMsg -->|"Dummy Vector (instant)"| QuantStore
    AddMsg -->|"setImmediate (background)"| MiniLM
    MiniLM -->|"updateLastVector()"| QuantStore
    
    QuantStore --> CoreKernelAuth
    CoreKernelAuth -->|"mintAuthToken()"| TensorEngine
    TensorEngine --> RoleIndex
    TensorEngine --> ECC
    QuantStore --> GC
    QuantStore --> JSONL

    HybridCtx -->|"Sliding Window (6 msgs)"| AddMsg
    HybridCtx -->|"Cosine Similarity RAG"| QuantStore
    LTM --> EncFile
    Profile --> ProfileJSON

    Sensory --> ActiveWin
    Sensory --> Clipboard

    classDef mem fill:#533483,stroke:#e94560,stroke-width:2px,color:#fff
    classDef store fill:#1a535c,stroke:#4ecdc4,stroke-width:2px,color:#fff
    classDef disk fill:#2d132c,stroke:#ee4540,stroke-width:2px,color:#fff

    class AddMsg,HybridCtx,LTM,Profile mem
    class CoreKernelAuth,TensorEngine,RoleIndex,ECC,GC store
    class JSONL,EncFile,ProfileJSON disk
```

---

## 4. Dual-Port AI Engine (Router ↔ Expert)

```mermaid
graph TD
    subgraph Gateway ["Node.js Gateway"]
        AgentLoop["AgentLoop"]
        DualPort["DualPortController"]
        Orchestrator["ModelOrchestrator"]
        RouterClient["OpenAI SDK (Router)"]
        ExpertClient["OpenAI SDK (Expert)"]
        NativeIPC["NativeIPCClient"]
    end

    subgraph LocalEngine ["Python AI Engines"]
        direction TB
        EngineHTTP["engine.py<br/>(llama-cpp-python)<br/>HTTP :8000"]
        EngineNative["liva_native_engine.py<br/>(ctypes → llama.dll)<br/>TCP :8100"]
    end

    subgraph ExpertServer ["Expert Server (On-demand)"]
        LlamaExpert["llama-server.exe<br/>:8001"]
    end

    subgraph GPU ["GPU VRAM"]
        VRAM["VRAM Pool"]
    end

    subgraph CloudAPI ["☁️ Cloud API (Optional)"]
        Gemini["Gemini / OpenAI API"]
    end

    AgentLoop -->|"Câu hỏi đơn giản"| RouterClient
    AgentLoop -->|"Native IPC Mode"| NativeIPC

    RouterClient -->|"HTTP :8000/v1"| EngineHTTP
    NativeIPC -->|"JSONL/TCP :8100"| EngineNative

    EngineHTTP --> VRAM
    EngineNative --> VRAM

    AgentLoop -->|"Handoff to Expert"| DualPort
    DualPort -->|"stopRouter()"| Orchestrator
    DualPort -->|"startExpert()"| Orchestrator
    Orchestrator -->|"spawn + Health Check"| LlamaExpert
    LlamaExpert --> VRAM

    AgentLoop -->|"Câu hỏi Expert"| ExpertClient
    ExpertClient -->|"HTTP :8001/v1"| LlamaExpert
    ExpertClient -.->|"Cloud Mode"| Gemini

    DualPort -->|"releaseResources()"| Orchestrator
    Orchestrator -->|"treeKill → xả VRAM"| LlamaExpert
    Orchestrator -->|"resume_peripherals"| AgentLoop

    classDef gw fill:#0f3460,stroke:#16213e,stroke-width:2px,color:#fff
    classDef py fill:#2d132c,stroke:#ee4540,stroke-width:2px,color:#fff
    classDef gpu fill:#0d7377,stroke:#14ffec,stroke-width:2px,color:#fff
    classDef cloud fill:#1a1a2e,stroke:#e94560,stroke-width:2px,color:#fff

    class AgentLoop,DualPort,Orchestrator,RouterClient,ExpertClient,NativeIPC gw
    class EngineHTTP,EngineNative py
    class LlamaExpert,VRAM gpu
    class Gemini cloud
```

---

## 5. Cấu Trúc Thư Mục (Directory Map)

```mermaid
graph LR
    Root["openclaw_remake/"]

    Root --> UIDir["liva-ui/"]
    Root --> GWDir["openclaw-gateway/"]
    Root --> EngDir["liva-ai-engine/"]
    Root --> DataDir["data/"]
    Root --> DocsDir["docs/"]

    UIDir --> UISrc["src/"]
    UISrc --> AppVue2["App.vue"]
    UISrc --> Comps["components/ (VoiceChat, HelloWorld)"]
    UIDir --> Electron["electron.cjs"]

    GWDir --> GWSrc["src/"]
    GWSrc --> GatewayTS["Gateway.ts (Entry)"]
    GWSrc --> CoreDir["core/ (7 files)"]
    CoreDir --> CK2["CoreKernel.ts"]
    CoreDir --> AL2["AgentLoop.ts (774 lines)"]
    CoreDir --> MO2["ModelOrchestrator.ts"]
    CoreDir --> PB2["PromptBuilder.ts"]
    CoreDir --> UI2["UIController.ts"]
    CoreDir --> ZP2["ZaloPolling.ts"]

    GWSrc --> MemDir["memory/ (5 files)"]
    MemDir --> TQS["TurboQuantStore.ts (493 lines)"]
    MemDir --> SM["SensoryManager.ts"]
    MemDir --> HC["HeraCompass.ts"]
    MemDir --> LM["LanceMemory.ts"]

    GWSrc --> SkillDir["skills/ (27 files)"]
    GWSrc --> UtilDir["utils/ (11 files)"]
    UtilDir --> NIPC["NativeIPCClient.ts (403 lines)"]
    UtilDir --> Logger["logger.ts"]
    UtilDir --> LivaEng["LivaEngine.ts"]

    GWSrc --> SecDir["security/ (ZMAS_Guard)"]
    GWSrc --> EvoDir["evolution/ (5 files)"]
    GWSrc --> SvcDir["services/ (Voice, Whisper)"]
    GWSrc --> SandDir["sandbox/ (MicroVMDaemon)"]
    GWSrc --> DepDir["deployment/ (BlueGreenRouter)"]
    GWSrc --> AutoSing["auto_singularity.ts (572 lines)"]

    GWSrc --> SkillReg["SkillRegistry.ts"]
    GWSrc --> MemMgr["MemoryManager.ts"]

    EngDir --> EngPy["engine.py (Uvicorn HTTP)"]
    EngDir --> NatPy["liva_native_engine.py (ctypes)"]
    EngDir --> VoicePy["voice_engine.py"]
    EngDir --> LlamaExe["llama-server.exe (16MB)"]

    classDef dir fill:#16213e,stroke:#0f3460,stroke-width:2px,color:#fff
    classDef file fill:#1a535c,stroke:#4ecdc4,stroke-width:1px,color:#fff
    classDef important fill:#e94560,stroke:#0f3460,stroke-width:2px,color:#fff

    class Root,UIDir,GWDir,EngDir,DataDir,DocsDir,UISrc,GWSrc,CoreDir,MemDir,SkillDir,UtilDir,SecDir,EvoDir,SvcDir,SandDir,DepDir,Comps dir
    class AppVue2,Electron,GatewayTS,SkillReg,MemMgr,AutoSing,EngPy,NatPy,VoicePy,LlamaExe,Logger,LivaEng,SM,HC,LM file
    class CK2,AL2,MO2,PB2,UI2,ZP2,TQS,NIPC important
```

---

## 6. Voice Pipeline (Giọng Nói ↔ AI)

```mermaid
sequenceDiagram
    actor User as 👤 Người dùng
    participant Mic as 🎤 Microphone
    participant UI as Liva UI
    participant WS as UIController (WS)
    participant Whisper as WhisperNode (STT)
    participant CK as CoreKernel
    participant AL as AgentLoop
    participant Voice as VoiceEngine (TTS)
    participant Speaker as 🔊 Speaker

    User->>Mic: Nói
    Mic->>UI: Audio buffer
    UI->>WS: Binary WebSocket frame
    WS->>Whisper: emit("audio_input", buffer)
    Whisper->>Whisper: pushAudioChunk() → Transcribe
    Whisper->>CK: emit("transcription_ready", text)
    CK->>AL: dispatch("agent_input", text)

    Note over AL: Xử lý AI (xem Diagram #2)

    AL->>Voice: pushTokens(chunk) via onStreamChunk
    Voice->>Voice: edge-tts → Base64 MP3
    Voice->>CK: emit("audio_base64", base64)
    CK->>WS: broadcastUIEvent("ai_audio_chunk")
    WS->>UI: WebSocket {audio: base64}
    UI->>UI: Web Audio API decode + queue
    UI->>Speaker: Phát âm thanh

    Note over UI: Live2D avatar animation<br/>đồng bộ với audio output
```

---

## 7. Auto-Singularity (Chu Kỳ Tự Tiến Hóa)

```mermaid
graph TD
    Start(["♾️ Infinite Loop"])

    Start --> Kill8000["Kill Router E4B (:8000)<br/>Xả VRAM"]
    Kill8000 --> StartPlanner["Start Planner (:8001)<br/>26B Expert Model"]
    StartPlanner --> ScanCode["extractProjectSurface()<br/>Quét Top 10 file nặng nhất"]
    ScanCode --> WebSearch["robustWebSearch()<br/>DuckDuckGo cắt kiến thức"]
    WebSearch --> RAGAxioms["LanceDB RAG<br/>Nạp Tiên Đề cũ"]
    RAGAxioms --> PlannerAI["Planner AI (26B)<br/>Sinh JSON Kế Hoạch"]
    
    PlannerAI --> CoderAI["Coder AI (tái dùng :8001)<br/>Skill: liva_ai_scientist"]
    CoderAI --> Sandbox["DockerSandbox<br/>Biên dịch + Test"]
    
    Sandbox -->|"✅ Pass"| Merge["Merge code vào source"]
    Sandbox -->|"❌ Fail"| Rollback["Rollback .bak"]

    Merge --> Distill["distillKnowledge()<br/>Chưng cất 15 Axioms"]
    Rollback --> Distill
    Distill --> LanceDB["Lưu vào LanceDB"]

    LanceDB --> Restore["Kill :8001 :8002<br/>Khởi lại Router E4B (:8000)"]
    Restore --> ZaloSOS["Gửi Zalo SOS<br/>(nếu có lỗi)"]
    ZaloSOS --> Cooldown["⏳ Nghỉ 60s"]
    Cooldown --> Start

    classDef phase fill:#533483,stroke:#e94560,stroke-width:2px,color:#fff
    classDef action fill:#0f3460,stroke:#16213e,stroke-width:2px,color:#fff
    classDef result fill:#1a535c,stroke:#4ecdc4,stroke-width:2px,color:#fff

    class Start,Cooldown phase
    class Kill8000,StartPlanner,ScanCode,WebSearch,RAGAxioms,PlannerAI,CoderAI,Sandbox action
    class Merge,Rollback,Distill,LanceDB,Restore,ZaloSOS result
```

---

## Chú Thích Màu Sắc

| Màu | Ý nghĩa |
|-----|---------|
| 🔴 Đỏ viền | File cốt lõi quan trọng nhất |
| 🔵 Xanh dương | Core Gateway modules |
| 🟣 Tím | Memory / Storage layer |
| 🟢 Xanh lá | Skills / Plugins |
| 🟤 Nâu đỏ | Python AI Engine |
| 🔵 Cyan | GPU / Model layer |

---

## Thống Kê Nhanh

| Thành phần | Số file | Ghi chú |
|-----------|---------|---------|
| **openclaw-gateway/src/core/** | 7 file | Lõi xử lý chính |
| **openclaw-gateway/src/skills/** | 27 file | Plugin kỹ năng |
| **openclaw-gateway/src/memory/** | 5 file | Bộ nhớ đa tầng |
| **openclaw-gateway/src/utils/** | 11 file | Tiện ích hạ tầng |
| **openclaw-gateway/src/evolution/** | 5 file | Tự tiến hóa |
| **liva-ai-engine/** | 3 file chính | Python inference |
| **liva-ui/src/** | 3 file | Vue + Electron |
| **Tổng Lines of Code** | ~5000+ | Không kể node_modules |
