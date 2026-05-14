# 🏗️ LIVA — Codebase Architecture Diagram

> Mở file này trong VS Code, chuột phải → **"Preview Mermaid"** để xem sơ đồ trực quan.

---

## 1. Tổng Quan Kiến Trúc Hệ Thống (System Overview)

```mermaid
graph TD
    User([👤 Người dùng])

    subgraph UI ["🖥️ liva-ui (Vue 3 + Tauri v2 / Rust)"]
        AppVue["App.vue"]
        VoiceChat["VoiceChat.vue"]
        WebWorkers["Web Workers (Audio/Live2D)"]
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
            ToolOrch["ToolExecutionOrchestrator"]
            LTCOrch["LTCOrchestrator"]
            TaskLaneWorker["TaskLaneWorker (Pub/Sub)"]
            MicroVM["MicroVMDaemon (WASI/isolated-vm)"]
        end

        subgraph Memory ["💾 Memory"]
            MemoryManager["MemoryManager"]
            StructuredMemory["StructuredMemory (sqlite-vec + FTS5)"]
            RamCache["RamCacheManager"]
            SensoryManager["SensoryManager"]
            HeraCompass["HeraCompass"]
            ReflectionDaemon["ReflectionDaemon"]
        end

        subgraph Skills ["🔧 Skills (78+ plugins)"]
            LocalMCPServer["LocalMCPServer (In-process MCP)"]
            WebSearch["WebSearch"]
            WebBrowser["WebBrowser"]
            SendZaloBot["SendZaloBot"]
            ReadEmails["ReadEmails"]
            ExecuteCommand["ExecuteCommand"]
            GitSync["GitSyncProject"]
            AIScientist["AIScientist"]
            FileOps["Read/Write/Delete Files"]
        end

        subgraph Security ["🔒 Security"]
            ZMASGuard["ZMAS_Guard"]
        end

        subgraph Evolution ["🧬 Singularity Pipeline"]
            EvolutionPipeline["EvolutionPipeline"]
            ASTCodeSurgeon["ASTCodeSurgeon"]
            RollbackManager["RollbackManager"]
            GitNexusIndexer["GitNexusIndexer"]
        end

        subgraph Infra ["🏭 Infrastructure"]
            MCPClientManager["MCPClientManager"]
            NativeIPCClient["NativeIPCClient (TCP:8100)"]
            ShieldGuard["ShieldGuard"]
            BlueGreenRouter["BlueGreenRouter"]
            PluginSDK["PluginSDK"]
        end

        subgraph Services ["🎙️ Services"]
            VoiceEngine["VoiceEngine (edge-tts)"]
            WhisperNode["WhisperNode (STT)"]
        end

        AutoSingularity["auto_singularity.ts (Tự Tiến Hóa)"]
    end

    subgraph Engine ["⚙️ liva-ai-engine (C++ / Python)"]
        EngineNative["liva_native_engine.py (ctypes)"]
        VoiceEnginePy["voice_engine.py (edge-tts)"]
        LlamaServer["llama-server (C++)"]
    end

    subgraph Models ["🧊 AI Models (E:/AI_Models)"]
        SingleExpertModel["Single Expert Model (100% VRAM)"]
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
    AgentLoop --> ToolOrch
    AgentLoop --> LTCOrch
    AgentLoop --> TaskLaneWorker

    PromptBuilder --> MemoryManager
    PromptBuilder --> SensoryManager

    ToolOrch --> MCPClientManager
    MCPClientManager --> LocalMCPServer
    ToolOrch --> ZMASGuard
    LTCOrch --> MemoryManager

    %% === AI Inference ===
    AgentLoop -->|"OpenAI SDK / HTTP :8000"| LlamaServer
    AgentLoop -->|"JSONL/TCP :8100"| NativeIPCClient
    NativeIPCClient -->|"TCP Socket"| EngineNative
    ModelOrchestrator -->|"spawn & Health Check"| LlamaServer
    LlamaServer --> SingleExpertModel

    %% === Memory ===
    MemoryManager --> StructuredMemory
    StructuredMemory -->|"sqlite-vec / FTS5"| RamCache

    %% === Skills ===
    LocalMCPServer --> WebSearch
    LocalMCPServer --> SendZaloBot
    LocalMCPServer --> ExecuteCommand
    LocalMCPServer --> AIScientist
    LocalMCPServer --> FileOps

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
    AppVue --> WebWorkers

    %% === Singularity ===
    AutoSingularity --> EvolutionPipeline
    EvolutionPipeline --> ASTCodeSurgeon
    EvolutionPipeline --> RollbackManager
    EvolutionPipeline --> LocalMCPServer
    EvolutionPipeline --> StructuredMemory

    %% === Styling ===
    classDef ui fill:#1a1a2e,stroke:#e94560,stroke-width:2px,color:#fff
    classDef core fill:#0f3460,stroke:#16213e,stroke-width:2px,color:#fff
    classDef memory fill:#533483,stroke:#e94560,stroke-width:2px,color:#fff
    classDef skill fill:#1a535c,stroke:#4ecdc4,stroke-width:2px,color:#fff
    classDef engine fill:#2d132c,stroke:#ee4540,stroke-width:2px,color:#fff
    classDef model fill:#0d7377,stroke:#14ffec,stroke-width:2px,color:#fff
    classDef security fill:#6a0572,stroke:#ab4e68,stroke-width:2px,color:#fff

    class AppVue,VoiceChat,WebWorkers,AudioPlayer ui
    class Gateway,CoreKernel,AgentLoop,ModelOrchestrator,PromptBuilder,UIController,ZaloPolling,TelemetryProfiler core
    class MemoryManager,StructuredMemory,RamCache,SensoryManager,HeraCompass,ReflectionDaemon memory
    class LocalMCPServer,WebSearch,WebBrowser,SendZaloBot,ReadEmails,ExecuteCommand,GitSync,AIScientist,FileOps skill
    class EngineNative,VoiceEnginePy,LlamaServer engine
    class SingleExpertModel model
    class ZMASGuard,ShieldGuard security
```

---

## 2. Luồng Xử Lý Tin Nhắn (Message Flow)

```mermaid
sequenceDiagram
    actor User as 👤 Người dùng
    participant UI as Liva UI (Vue / Tauri)
    participant WS as UIController (WS:8082)
    participant CK as CoreKernel
    participant AL as AgentLoop
    participant PB as PromptBuilder
    participant MM as MemoryManager
    participant AI as AI Engine (Single Expert)
    participant MCP as LocalMCPServer
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
    PB->>PB: buildToolsPrompt() (Semantic Filter sqlite-vec)
    PB-->>AL: Full AI Messages array

    AL->>AI: chat.completions.create({stream: true})
    
    loop Streaming tokens
        AI-->>AL: token chunk
        AL->>WS: onStreamChunk → broadcastUIEvent
        WS-->>UI: {ai_stream_chunk}
        UI->>UI: Cập nhật bubble + Live2D animation (via Web Workers)
    end

    alt AI gọi Tool (XML <tool_call>)
        AL->>AL: Parse XML → toolCalls[]
        AL->>TO: executeWithReflection(toolName, args)
        TO->>MCP: executeSkill()
        MCP-->>TO: Kết quả
        TO->>TO: ZMAS_Guard.autoRemediation()
        TO->>TO: Reflection Layer (heuristic)
        TO-->>AL: {resultStr, valid, rawObj}
        
        Note over AL: Nạp kết quả tool vào<br/>context → Lặp lại AI call
    end

    AL->>MM: addMessage("user", text)
    AL->>MM: addMessage("assistant", reply)
    AL->>AL: LTCOrchestrator.summarizeAndStore()
    AL->>WS: onSpokenResponse(finalReply)
    WS-->>UI: {ai_spoken_response}
```

---

## 3. Kiến Trúc Bộ Nhớ (Memory Architecture - Single SQLite)

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

    subgraph StructuredMemory ["StructuredMemory (node:sqlite)"]
        direction TB
        FTS5["FTS5 (Full-text Search)"]
        SqliteVec["sqlite-vec (Vector Search)"]
        EventsTable["turn_layer_nodes / events"]
        KVStore["L3 KV Facts"]
    end

    subgraph BackgroundDaemons ["Background Daemons"]
        ReflectionDaemon["ReflectionDaemon (Φ/Ψ extraction)"]
        ConsolidationCron["ConsolidationCron (L1 → L2/L3)"]
    end

    subgraph Storage ["Disk Storage"]
        SQLiteDB["StructuredMemory.sqlite"]
        EncFile["LIVA Vault (AES-256-GCM)"]
    end

    subgraph Sensory ["SensoryManager (Singleton)"]
        ActiveWin["active-win"]
        Clipboard["clipboardy"]
        TTL["TTL: 30s auto-expire"]
    end

    UserMsg --> AddMsg
    AIReply --> AddMsg
    AddMsg -->|"Debounced Writes"| StructuredMemory
    
    StructuredMemory --> FTS5
    StructuredMemory --> SqliteVec
    StructuredMemory --> EventsTable
    StructuredMemory --> KVStore

    HybridCtx -->|"Sliding Window (6 msgs)"| AddMsg
    HybridCtx -->|"Cosine Similarity RAG"| SqliteVec
    LTM --> SQLiteDB
    Profile --> SQLiteDB

    ReflectionDaemon -->|"Extracts from Events"| StructuredMemory
    ConsolidationCron -->|"Synthesizes"| StructuredMemory

    StructuredMemory --> SQLiteDB

    Sensory --> ActiveWin
    Sensory --> Clipboard

    classDef mem fill:#533483,stroke:#e94560,stroke-width:2px,color:#fff
    classDef store fill:#1a535c,stroke:#4ecdc4,stroke-width:2px,color:#fff
    classDef disk fill:#2d132c,stroke:#ee4540,stroke-width:2px,color:#fff

    class AddMsg,HybridCtx,LTM,Profile mem
    class StructuredMemory,FTS5,SqliteVec,EventsTable,KVStore store
    class SQLiteDB,EncFile disk
```

---

## 4. Single Expert AI Engine (Adaptive Mode)

```mermaid
graph TD
    subgraph Gateway ["Node.js Gateway"]
        AgentLoop["AgentLoop"]
        Orchestrator["ModelOrchestrator"]
        AIClient["OpenAI SDK"]
    end

    subgraph LocalEngine ["Local Engine (C++)"]
        LlamaServer["llama-server.exe<br/>:8000"]
    end

    subgraph GPU ["GPU VRAM"]
        VRAM["100% VRAM Pool"]
    end

    subgraph CloudAPI ["☁️ Cloud API"]
        Gemini["Gemini / OpenAI API / Claude"]
    end

    AgentLoop -->|"Query"| AIClient
    Orchestrator -->|"Auto-detect Hardware"| LlamaServer
    
    AIClient -.->|"Local Mode"| LlamaServer
    AIClient -.->|"Cloud/Hybrid Mode"| Gemini

    LlamaServer --> VRAM

    classDef gw fill:#0f3460,stroke:#16213e,stroke-width:2px,color:#fff
    classDef py fill:#2d132c,stroke:#ee4540,stroke-width:2px,color:#fff
    classDef gpu fill:#0d7377,stroke:#14ffec,stroke-width:2px,color:#fff
    classDef cloud fill:#1a1a2e,stroke:#e94560,stroke-width:2px,color:#fff

    class AgentLoop,Orchestrator,AIClient gw
    class LlamaServer py
    class VRAM gpu
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
    Root --> DocsDir["docs/"]

    UIDir --> UISrc["src/"]
    UISrc --> AppVue2["App.vue"]
    UISrc --> Comps["components/ (VoiceChat, HelloWorld)"]
    UIDir --> TauriDir["src-tauri/"]

    GWDir --> GWSrc["src/"]
    GWSrc --> GatewayTS["Gateway.ts (Entry)"]
    GWSrc --> CoreDir["core/ (14 files)"]
    CoreDir --> CK2["CoreKernel.ts"]
    CoreDir --> AL2["AgentLoop.ts"]
    CoreDir --> MO2["ModelOrchestrator.ts"]
    CoreDir --> PB2["PromptBuilder.ts"]
    CoreDir --> UI2["UIController.ts"]
    CoreDir --> ZP2["ZaloPolling.ts"]

    GWSrc --> MemDir["memory/ (8 files)"]
    MemDir --> SM["SensoryManager.ts"]
    MemDir --> HC["HeraCompass.ts"]
    MemDir --> SMem["StructuredMemory.ts"]
    MemDir --> RD["ReflectionDaemon.ts"]

    GWSrc --> SkillDir["skills/ (78+ files)"]
    GWSrc --> MCPDir["mcp/ (LocalMCPServer)"]
    GWSrc --> UtilDir["utils/ (HttpClient, logger)"]

    GWSrc --> SecDir["security/ (ZMAS_Guard)"]
    GWSrc --> EvoDir["evolution/ (ASTCodeSurgeon)"]
    GWSrc --> SvcDir["services/ (VoiceEngine)"]
    GWSrc --> SandDir["sandbox/ (MicroVMDaemon)"]
    GWSrc --> AutoSing["auto_singularity.ts"]

    GWSrc --> MemMgr["MemoryManager.ts"]

    EngDir --> VoicePy["voice_engine.py"]
    EngDir --> LlamaExe["llama-server (C++)"]

    classDef dir fill:#16213e,stroke:#0f3460,stroke-width:2px,color:#fff
    classDef file fill:#1a535c,stroke:#4ecdc4,stroke-width:1px,color:#fff
    classDef important fill:#e94560,stroke:#0f3460,stroke-width:2px,color:#fff

    class Root,UIDir,GWDir,EngDir,DocsDir,UISrc,GWSrc,CoreDir,MemDir,SkillDir,MCPDir,UtilDir,SecDir,EvoDir,SvcDir,SandDir,Comps,TauriDir dir
    class AppVue2,GatewayTS,MemMgr,AutoSing,VoicePy,LlamaExe,SM,HC,SMem,RD file
    class CK2,AL2,MO2,PB2,UI2,ZP2 important
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
    UI->>UI: Web Audio API decode + queue (Web Workers)
    UI->>Speaker: Phát âm thanh

    Note over UI: Live2D avatar animation<br/>đồng bộ với audio output (Web Workers)
```

---

## 7. Auto-Singularity (Chu Kỳ Tự Tiến Hóa)

```mermaid
graph TD
    Start(["♾️ Infinite Loop"])

    Start --> StartPlanner["Kích hoạt Planner AI<br/>Expert Model"]
    StartPlanner --> ScanCode["extractProjectSurface()<br/>Quét Top 10 file nặng nhất"]
    ScanCode --> RAGAxioms["SQLite-Vec RAG<br/>Nạp Tiên Đề cũ"]
    RAGAxioms --> PlannerAI["Planner AI<br/>Sinh JSON Kế Hoạch"]
    
    PlannerAI --> CoderAI["Coder AI<br/>Skill: liva_ai_scientist"]
    CoderAI --> AST["ASTCodeSurgeon<br/>Phẫu thuật AST & Atomic Write"]
    AST --> Sandbox["MicroVMDaemon<br/>Biên dịch + Test"]
    
    Sandbox -->|"✅ Pass"| Merge["Merge code vào source"]
    Sandbox -->|"❌ Fail"| Rollback["RollbackManager<br/>Phục hồi .bak snapshot"]

    Merge --> Distill["distillKnowledge()<br/>Chưng cất 15 Axioms"]
    Rollback --> Distill
    Distill --> SQLiteDB["Lưu vào StructuredMemory SQLite"]

    SQLiteDB --> ZaloSOS["Gửi Zalo SOS<br/>(nếu có lỗi)"]
    ZaloSOS --> Cooldown["⏳ Nghỉ 60s"]
    Cooldown --> Start

    classDef phase fill:#533483,stroke:#e94560,stroke-width:2px,color:#fff
    classDef action fill:#0f3460,stroke:#16213e,stroke-width:2px,color:#fff
    classDef result fill:#1a535c,stroke:#4ecdc4,stroke-width:2px,color:#fff

    class Start,Cooldown phase
    class StartPlanner,ScanCode,RAGAxioms,PlannerAI,CoderAI,AST,Sandbox action
    class Merge,Rollback,Distill,SQLiteDB,ZaloSOS result
```

---

## Chú Thích Màu Sắc

| Màu | Ý nghĩa |
|-----|---------|
| 🔴 Đỏ viền | File cốt lõi quan trọng nhất |
| 🔵 Xanh dương | Core Gateway modules |
| 🟣 Tím | Memory / Storage layer |
| 🟢 Xanh lá | Skills / Plugins |
| 🟤 Nâu đỏ | Engine (C++ / Python) |
| 🔵 Cyan | GPU / Model layer |

---

## Thống Kê Nhanh

| Thành phần | Số file | Ghi chú |
|-----------|---------|---------|
| **openclaw-gateway/src/core/** | 14 file | Lõi xử lý chính |
| **openclaw-gateway/src/skills/** | 78+ file | Plugin kỹ năng |
| **openclaw-gateway/src/memory/** | 8 file | Bộ nhớ đa tầng (SQLite) |
| **openclaw-gateway/src/utils/** | 11 file | Tiện ích hạ tầng |
| **openclaw-gateway/src/evolution/** | 5 file | Tự tiến hóa |
| **liva-ai-engine/** | 2 file chính | C++ & Python inference/TTS |
| **liva-ui/src/** | 3 file | Vue + Tauri |
| **Tổng Lines of Code** | ~6000+ | Không kể node_modules |
