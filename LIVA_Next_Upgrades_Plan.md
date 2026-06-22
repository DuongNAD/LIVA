# LIVA Strategic Upgrade & Optimization Plan — Phase D & Phase E

## 1. Executive Summary & Codebase Quality Status

Following the successful completion of **Phase A** (Banned dependency removal & typesafety fixes), **Phase B** (ESLint warning remediation & logger migration), and **Phase C** (God component decoupling), the LIVA system is in a stable compilation and linter state. 

To determine the next logical steps for LIVA, a comprehensive full-stack research scan was conducted across the entire codebase—including the Frontend UI (`liva-ui`), Desktop Shell (`liva-desktop`), Backend Gateway (`liva-gateway`), and AI & Voice Engine (`liva-ai-engine`, `liva-dataset`).

This scan was conducted under **strict read-only research constraints** (no source code files modified). The audit identified critical areas of concern: database search latency under scaling metadata filters, IPC round-trip write amplification, frontend memory leaks, security vulnerabilities in Tauri's disabled Content Security Policy (CSP), unsafe cryptography in Stronghold key derivation, dynamic CDN resource dependencies hindering offline use, and artificial mocks/inefficiencies in local AI workers (TTS, STT, FlashRank, Moonshine, Florence-2).

This report outlines the proposed strategic roadmap:
- **Phase D**: Core Reliability, Database Optimization, and System Performance.
- **Phase E**: Advanced Local AI Capabilities, Client Security, and Offline Isolation.

---

## 2. Phase D: Core Reliability, Database Optimization, and System Performance

Phase D aims to maximize database performance, eliminate event loop blocking and memory leaks, and solidify backend and shell reliability.

### 2.1. Database Indexing & RAG Query Optimization
* **Evidence (File Paths & Line Numbers):**
  - **Table Definition**: `liva-gateway/src/memory/VectorRepository.ts` (lines 98-111) defines the metadata schema for `vectors_meta` storing properties like `type`, `domain`, and `category`.
  - **Query & Pre-Filtering**: `liva-gateway/src/memory/VectorRepository.ts` (lines 423-438) performs vector queries that filter by these metadata properties inside a subquery matching `v.rowid IN (SELECT id FROM vectors_meta WHERE ${metaConditions})`.
* **Problem:** There are no SQL indexes on `vectors_meta(type, domain, category)` or `vectors_meta(created_at)`. As the vector registry grows, SQLite is forced to execute sequential table scans to evaluate the subquery pre-filter, degrading RAG retrieval performance.
* **Proposed Upgrade:**
  - Create composite and single-column indexes during store initialization:
    ```sql
    CREATE INDEX IF NOT EXISTS idx_vectors_meta_filter ON vectors_meta(type, domain, category);
    CREATE INDEX IF NOT EXISTS idx_vectors_meta_created ON vectors_meta(created_at);
    ```

### 2.2. Batch Transactions & Write-Amplification Prevention
* **Evidence (File Paths & Line Numbers):**
  - **Event Consolidation**: `liva-gateway/src/memory/EventRepository.ts` (lines 154-185) loops through event IDs and updates their consolidation state sequentially:
    ```typescript
    const stmt = this.#db.prepare("UPDATE events SET consolidated = 1, consolidation_status = 'consolidated' WHERE eventId = ?");
    for (const id of eventIds) {
        await stmt.run(id);
    }
    ```
  - **Graph Upserts**: `liva-gateway/src/memory/ConsolidationSteps.ts` (lines 319-329) loops over extracted nodes and edges, invoking `await this.#deps.structuredMemory.graph.upsertNode(node)` and `await this.#deps.structuredMemory.graph.upsertEdge(edge)` in sequence.
* **Problem:** Each loop execution triggers an independent database promise/IPC message to the database worker thread. Furthermore, because they are not wrapped in a single database transaction, SQLite is forced to commit to the physical journal (disk write) for *each individual statement*, causing event-loop stalling and high write amplification.
* **Proposed Upgrade:**
  - Introduce batch transaction methods in `DatabaseWorkerBridge` (e.g. `runBatch()` or `transactionBatch()`).
  - Refactor all sequential database update loops in `EventRepository.ts` and `ConsolidationSteps.ts` to execute inside a single atomic transaction.

### 2.3. Memory Leak Remediation in Frontend Composables
* **Evidence (File Paths & Line Numbers):**
  - **useVRM.ts**: `liva-ui/src/composables/useVRM.ts` (lines 196-198) registers a listener on the global document:
    ```typescript
    document.addEventListener('visibilitychange', () => {
        isWindowVisible = !document.hidden;
    });
    ```
    This listener is never removed inside `dispose()` (lines 633-654).
  - **useVoicePipeline.ts**: `liva-ui/src/composables/useVoicePipeline.ts` (lines 350-351) registers autoplay interaction bypasses:
    ```typescript
    globalThis.document?.addEventListener('click', resumeContext);
    globalThis.document?.addEventListener('keydown', resumeContext);
    ```
    These are never cleaned up or removed in `stopPipeline()` (lines 392-440).
* **Problem:** Dangling event listeners hold closures in memory, preventing the components from being garbage-collected. This creates persistent memory leaks that accumulate whenever a user switches models or toggles the voice communication flow.
* **Proposed Upgrade:**
  - Track registered event listeners and cleanly call `removeEventListener` inside their respective cleanup blocks (`dispose()` in `useVRM.ts` and `stopPipeline()` in `useVoicePipeline.ts`).

### 2.4. AST Code Surgeon Paths Resolution
* **Evidence (File Paths & Line Numbers):**
  - **AST Surgery**: `liva-gateway/src/workers/ASTWorker.ts` (lines 216-227) initializes ts-morph's compiler wrapper:
    ```typescript
    const project = new Project({
        compilerOptions: { target: ScriptTarget.ESNext }
    });
    ```
* **Problem:** The compiler wrapper is instantiated in isolation without referencing the repository's `tsconfig.json`. When the target file imports modules via path aliases (e.g., `import { logger } from "@utils/Logger"`), the isolated compiler fails to resolve the path, generating false-positive diagnostics errors that block code self-healing.
* **Proposed Upgrade:**
  - Load the project configuration files during AST Surgery initialization:
    ```typescript
    const project = new Project({
        tsConfigFilePath: path.join(process.cwd(), "tsconfig.json"),
        skipAddingFilesFromTsConfig: true
    });
    ```

### 2.5. Tauri IPC Handler Registration
* **Evidence (File Paths & Line Numbers):**
  - **TauriAdapter.ts**: `liva-ui/src/platform/TauriAdapter.ts` (lines 47 & 58) calls Rust-side IPC functions:
    ```typescript
    return await invoke<string | null>('read_vault_key', { key });
    ...
    await invoke('write_vault_key', { key, value });
    ```
  - **lib.rs**: `liva-desktop/src-tauri/src/lib.rs` (line 136) defines registered command handlers:
    ```rust
    .invoke_handler(tauri::generate_handler![toggle_ghost_mode, update_interactive_zones, open_dashboard])
    ```
* **Problem:** The frontend attempts to store and retrieve sensitive configuration parameters (such as database credentials) using Tauri's Rust-side handlers, but `read_vault_key` and `write_vault_key` are never registered in the backend's invoke array, leading to silent runtime crashes when run in Tauri Desktop mode.
* **Proposed Upgrade:**
  - Implement and register the `read_vault_key` and `write_vault_key` functions inside `lib.rs` using standard Tauri state storage.

---

## 3. Phase E: Advanced Local AI Capabilities, Client Security, and Offline Isolation

Phase E focuses on upgrading model execution paths to GPU hardware, resolving mocked/simulated AI workflows, securing application boundaries, and achieving complete offline self-containment.

### 3.1. Local TTS Real-Time Chunk Streaming
* **Evidence (File Paths & Line Numbers):**
  - **TTS Server**: `liva-ai-engine/voice_engine.py` (lines 147-156):
    ```python
    communicate = edge_tts.Communicate(text, voice_to_use, rate="+15%")
    async for chunk in communicate.stream():
        if chunk["type"] == "audio" and "data" in chunk:
            audio_data.extend(chunk["data"])
            
    if len(audio_data) > 0:
        b64_audio = base64.b64encode(audio_data).decode("utf-8")
        await websocket.send_text(json.dumps({
            "type": "audio",
            "data": b64_audio
        }))
    ```
* **Problem:** Although the Edge-TTS library generates audio incrementally via standard generators, the backend voice engine aggregates the entire sentence into memory and only transmits it *after* generation completes. This adds substantial latency (500ms to 2s) before the client starts speaking (Time-To-First-Sound).
* **Proposed Upgrade:**
  - Update `voice_engine.py` to encode and push individual chunks over WebSocket immediately as they are yielded by the generator, and adapt the frontend audio queue to play them sequentially.

### 3.2. Local STT Sample Rate Validation & Resampling
* **Evidence (File Paths & Line Numbers):**
  - **STT Decodes**: `liva-ai-engine/whisper_stt_server.py` (lines 116-127):
    ```python
    if audio_bytes[:4] == b'RIFF':
        try:
            with wave.open(io.BytesIO(audio_bytes)) as wav:
                frames = wav.readframes(wav.getnframes())
                if wav.getsampwidth() == 2:
                    audio_array = np.frombuffer(frames, dtype=np.int16).astype(np.float32) / 32768.0
    ```
* **Problem:** The server parses incoming audio WAV blobs into raw float arrays but does not inspect or validate the source sampling rate (`wav.getframerate()`). If a client records audio at 44.1kHz or 48kHz, it is fed to the Whisper model without resampling, resulting in corrupted transcriptions.
* **Proposed Upgrade:**
  - Read the sample rate from the WAV header and apply a resampling utility (e.g. `scipy.signal.resample` or `soxr`) to convert any non-16kHz inputs to exactly 16kHz before inference.

### 3.3. WASM Asset Pruning and Dependency Clean-up
* **Evidence (File Paths & Line Numbers):**
  - **Obsolete Binaries**: The directory `liva-ui/public/` contains several large ONNX WASM runtimes:
    - `ort-wasm-simd-threaded.jsep.wasm` (19.5 MB)
    - `ort-wasm-simd.jsep.wasm` (17.4 MB)
    - `ort-wasm-simd-threaded.wasm` (10.6 MB)
    - `ort-wasm-simd.wasm` (10.5 MB)
    - `ort-wasm-threaded.wasm` (9.8 MB)
    - `ort-wasm.wasm` (9.7 MB)
    *(Total size: ~77.5 MB)*
  - **Wake Word Worker**: `liva-ui/src/workers/LivaWakeWorker.ts` (lines 64-74) executes native JavaScript feedforward loops using raw JSON weights, completely bypassing ONNX Web.
  - **package.json**: `liva-ui/package.json` (line 20) lists `"onnxruntime-web": "1.17"`.
* **Problem:** The 77.5MB WASM files and `onnxruntime-web` dependency are dead weight and are never loaded by the frontend, bloated the client build package unnecessarily.
* **Proposed Upgrade:**
  - Delete all `ort-wasm-*.wasm` files from `liva-ui/public/`.
  - Uninstall `onnxruntime-web` from `liva-ui/package.json`.

### 3.4. Localization of CDN Assets (Offline Isolation)
* **Evidence (File Paths & Line Numbers):**
  - **Live2D Widget**: `liva-ui/widget.html` (line 13) loads scripts from Fastly CDN:
    ```html
    <script src="https://fastly.jsdelivr.net/gh/stevenjoezhang/live2d-widget@latest/live2d.min.js"></script>
    ```
  - **Mediapipe Assets**: `liva-ui/src/composables/useFaceTracking.ts` (lines 200-208) loads face tracking resolvers and model tasks from online Google/jsDelivr CDNs:
    ```typescript
    const vision = await FilesetResolver.forVisionTasks("https://cdn.jsdelivr.net/npm/@mediapipe/tasks-vision@latest/wasm");
    ...
    modelAssetPath: "https://storage.googleapis.com/mediapipe-models/face_landmarker/.../face_landmarker.task"
    ```
* **Problem:** Loading critical libraries from external CDNs introduces security/XSS vulnerabilities, and breaks face-tracking and character loading whenever the client computer is offline.
* **Proposed Upgrade:**
  - Download `live2d.min.js`, Mediapipe WASM binaries, and the `face_landmarker.task` model file, and place them directly into `liva-ui/public/assets/` to load them locally.

### 3.5. FlashRank & Moonshine Workers Overhaul (Unlocking Real Models)
* **Evidence (File Paths & Line Numbers):**
  - **FlashRank Mock**: `liva-gateway/src/workers/FlashRankWorker.ts` (lines 55-66) creates a dummy tensor and intentionally throws an error:
    ```typescript
    const dummyInput = new ort.Tensor("int64", BigInt64Array.from([1n, 2n, 3n]), [1, 3]);
    if (dummyInput) {
        throw new Error("Detailed tokenizer/tensor mapping required for model inputs.");
    }
    ```
    This forces the system to run a fallback Jaccard metric, leaving the ONNX reranker completely unused.
  - **Moonshine Decoding**: `liva-gateway/src/workers/MoonshineWorker.ts` (line 135) formats generated token outputs without using a tokenizer:
    ```typescript
    const text = tokens.map(t => `t${t}`).join(" ");
    ```
    This outputs raw token IDs (e.g. `"t12 t45"`) instead of English text.
* **Proposed Upgrade:**
  - Implement a real Tokenizer in `FlashRankWorker.ts` to tokenize queries and documents into input IDs, attention masks, and type IDs, then execute true ONNX inference.
  - Integrate a text tokenizer vocabulary in `MoonshineWorker.ts` to decode token indices into readable string characters.

### 3.6. Tauri Client Security Hardening (CSP & Key Derivation)
* **Evidence (File Paths & Line Numbers):**
  - **tauri.conf.json**: `liva-desktop/src-tauri/tauri.conf.json` (lines 44-46) configures WebView security:
    ```json
    "security": {
        "csp": null
    }
    ```
  - **Stronghold Key**: `liva-desktop/src-tauri/src/lib.rs` (lines 71-78) derives the database vault password:
    ```rust
    .plugin(tauri_plugin_stronghold::Builder::new(|password| {
        let mut key = vec![0u8; 32];
        for (i, b) in password.as_bytes().iter().enumerate().take(32) {
            key[i] = *b;
        }
        key
    }).build())
    ```
* **Problem:** Disabling Content Security Policy exposes the Tauri shell to remote code execution (RCE) if any third-party script gets injected. Additionally, key derivation simply pads/truncates the password, resulting in highly weak keys that render the Stronghold vault vulnerable to brute-force attacks.
* **Proposed Upgrade:**
  - Define a strict local-only CSP in `tauri.conf.json`:
    ```json
    "csp": "default-src 'self'; script-src 'self' 'unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' ws://127.0.0.1:* http://127.0.0.1:*;"
    ```
  - Replace the custom padding logic in `lib.rs` with a cryptographically secure KDF like Argon2id or PBKDF2 with standard iterations.

### 3.7. Tauri Cursor Polling Thread CPU Optimization
* **Evidence (File Paths & Line Numbers):**
  - **Polling Thread**: `liva-desktop/src-tauri/src/lib.rs` (lines 94-131) runs a thread in a loop:
    ```rust
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(30));
            // Calls cursor_position(), inner_position(), scale_factor() and set_ignore_cursor_events()
        }
    });
    ```
* **Problem:** Invoking cross-thread WebView APIs and updating window cursor-ignore properties every 30ms causes noticeable baseline CPU usage and thread scheduling overhead in Windows.
* **Proposed Upgrade:**
  - Replace the background loop with event-driven tracking. Detect pointer position on the frontend (`pointermove` / `mouseenter` / `mouseleave`) and call Tauri's `set_ignore_cursor_events` only when crossing boundary coordinates.

---

## 4. Verification Schedule & Exit Criteria

Below is the proposed verification checklist for testing and deploying Phase D and Phase E:

| Phase | Milestone / Focus | Verification Command | Exit Criteria |
|---|---|---|---|
| **Phase D** | Database indexing, batch transaction updates, memory leak resolutions, and AST path fixes. | `npm run test` (in `liva-gateway`) & `npx vue-tsc --noEmit` (in `liva-ui`) | `0` TypeScript and Vue errors. All 2,618 tests pass cleanly. `EXPLAIN QUERY PLAN` confirms vector queries hit metadata indexes. |
| **Phase E** | Real ONNX reranking, voice streaming, offline isolation, and Tauri security configuration. | `npm run test` & manual verification of offline launch. | Live2D character loads 100% offline. Stronghold stores credentials securely with Argon2 keys. No external CDN loads. |

---

## 5. Pure Research Verification

The research team confirms that no source files (such as `.ts`, `.tsx`, `.py`, `.rs`, or `.yaml`) have been modified or committed during this strategic scan. The repository files remain unchanged, maintaining the zero-modification constraint of this audit phase.
