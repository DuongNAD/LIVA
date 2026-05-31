# 📋 LIVA System — Báo Cáo Nghiên Cứu Lỗi

**Ngày:** 2026-05-14  
**Tác giả:** Claude AI (Research Agent)  
**Mục đích:** Phân tích nguyên nhân gốc rễ và đề xuất hướng khắc phục cho 2 vấn đề nghiêm trọng

---

## 🔴 Vấn Đề 1: "Chat 2 Lần Liên Tiếp" — Tin Nhắn Dính Chùm

### 📌 Mô Tả Lỗi

Khi user gửi tin nhắn thứ 2 trong lúc LIVA đang generate trả lời tin thứ 1, hệ thống chặn tin thứ 2 và hiển thị "Liva đang bận một chút..." nhưng luồng stream của tin 1 không bị hủy bỏ. Kết quả là cả hai nội dung bị gộp vào cùng một bong bóng chat tạo ra câu nói chắp vá.

---

### 🔬 Phân Tích Nguyên Nhân Gốc Rễ

#### Chain Of Events (chuỗi sự kiện)

```
T0: User gửi tin nhắn 1 ("Xin chào Liva")
T1: AgentLoop.handleUserInput() → isBusy = true
T2: generateText() bắt đầu stream
T3: User gửi tin nhắn 2 ("trời nay như nào")
T4: AgentLoop.handleUserInput() thấy isBusy = true
T5: Gọi this.onSpokenResponse("Liva đang bận một chút, xin anh đợi xíu nhé.")
    ↓
    ReactiveSync.ts:55 → dispatch("ai_spoken_response", { text })
    ↓
    UIController.ts:279 → broadcastUIEvent()
    ↓
    VoiceChat.vue:62-63 → currentAiText += "Liva đang bận..."
T6: ⚠️ Stream của tin nhắn 1 vẫn tiếp tục chạy (không bị abort)
T7: Tiếp tục stream → currentAiText += "ạ! 😊 Rất vui được gặp lại..."
```

#### Root Cause Analysis

| Thành Phần | Dòng Code | Vấn Đề |
|------------|-----------|---------|
| `AgentLoop.ts` | 202-209 | Khi `isBusy === true`, chỉ gọi `onSpokenResponse()` nhưng **KHÔNG** abort stream hiện tại |
| `AgentLoop.ts` | 390-393 | `AbortController` chỉ được kiểm tra khi user nói chen vào (barge-in), không phải khi có user input mới |
| `ReactiveSync.ts` | 55-66 | `onSpokenResponse` gửi thẳng text vào UI mà không phân biệt loại thông điệp |

#### Root Cause Chính

**AgentLoop thiếu logic abort stream khi nhận tin nhắn mới trong khi đang busy.**

Luồng hiện tại:
1. `handleUserInput()` kiểm tra `isBusy` → block tin 2
2. Gọi `onSpokenResponse("Liva đang bận...")` → gửi thẳng vào bong bóng chat
3. **Stream của tin 1 vẫn tiếp tục** → nối tiếp vào cùng bong bóng

---

### 📂 Code Liên Quan

#### AgentLoop.ts (Dòng 201-210)
```startLine:201:endLine:210:openclaw-gateway\src\core\AgentLoop.ts
    public handleUserInput(userText: string, isHeartbeat: boolean = false) {
        if (this.isBusy) {
            if (isHeartbeat) {
                logger.info(`[Heartbeat] ⚠️ Bỏ qua nhịp đập do AgentLoop đang bận.`);
                return;
            }
            logger.warn(`⚠️ Hệ thống đang bận xử lý tác vụ khác. Chặn: ${userText.substring(0, 50)}`);
            if (this.onSpokenResponse) this.onSpokenResponse("Liva đang bận một chút, xin anh đợi xíu nhé.");
            return;
        }
```

**Vấn đề:** Không gọi `bargeIn()` để abort stream trước khi gửi "đang bận".

#### ReactiveSync.ts (Dòng 55-66)
```startLine:55:endLine:66:openclaw-gateway\src\core\events\ReactiveSync.ts
    agentLoop.onSpokenResponse = async (text: string) => {
        if (text.trim() === "HEARTBEAT_OK" || text.includes("HEARTBEAT_OK")) {
            logger.info(`[Heartbeat] 🤫 Nhịp đập ổn định. Đã triệt tiêu âm thanh.`);
            return;
        }
        // [P5] Flush TTSFormatter buffer — gửi nốt câu cuối còn sót trong bộ đệm
        getVoiceEngine()?.flushTTS();
        await dispatch("ui_broadcast", {
            name: "ai_spoken_response",
            data: { text }
        });
    };
```

**Vấn đề:** Không phân biệt giữa "final response" và "system notification" (đang bận).

---

### 🛠️ Đề Xuất Khắc Phục

#### **Cách A: Chặn Thân Thiện (Dễ Nhất)**

Sửa `VoiceChat.vue` để khi nhận được sự kiện hệ thống báo bận, hiển thị **System Toast** thay vì đẩy vào bong bóng thoại của LIVA.

**Ưu điểm:** Đơn giản, không cần sửa backend
**Nhược điểm:** Stream của tin 1 vẫn tiếp tục chạy ngầm (tốn GPU)

#### **Cách B: Ngắt Luồng Triệt Để (Khuyến nghị)**

Thay vì chặn tin thứ 2, LIVA sẽ **Hủy (Abort) ngay lập tức** luồng sinh text của tin 1 và bắt đầu xử lý tin 2. Hành vi này giống ChatGPT.

**Thay đổi cần thiết:**

1. **AgentLoop.ts** - Thêm gọi `bargeIn()` trước khi gửi "đang bận":
```startLine:201:endLine:220:openclaw-gateway\src\core\AgentLoop.ts
    public handleUserInput(userText: string, isHeartbeat: boolean = false) {
        if (this.isBusy) {
            if (isHeartbeat) {
                logger.info(`[Heartbeat] ⚠️ Bỏ qua nhịp đập do AgentLoop đang bận.`);
                return;
            }
            // [FIX] Abort current stream before sending busy message
            this.bargeIn();
            // Stop TTS immediately
            voiceEngine?.preempt?.();
            
            logger.warn(`⚠️ Hệ thống đang bận. Tin nhắn mới ưu tiên. Chặn: ${userText.substring(0, 50)}`);
            // Thay vì gửi "đang bận", hãy xử lý tin nhắn mới luôn
            // Hoặc gửi event "system_busy" để UI hiển thị toast
            if (this.onSpokenResponse) this.onSpokenResponse("Liva đang bận một chút, xin anh đợi xíu nhé.");
            return;
        }
```

2. **VoiceChat.vue** - Thêm xử lý event `system_busy`:
```startLine:60:endLine:73:liva-ui\src\components\VoiceChat.vue
      ws.onmessage = async (event) => {
        const data = JSON.parse(event.data);
        
        // Xử lý System Notification (hiển thị toast thay vì bong bóng chat)
        if (data.type === 'system_busy') {
          this.showBusyToast(data.message); // Toast nhỏ trên UI
          return;
        }
        
        if (data.type === 'text') {
          currentAiText.value += data.text; // Cập nhật stream text
        }
```

---

### ✅ Hành Vi Mong Muốn (Cách B)

```
T0: User gửi tin 1 → LIVA bắt đầu suy nghĩ
T1: User gửi tin 2 → LIVA abort stream 1 → hiển thị toast "đang bận"
T2: LIVA bắt đầu xử lý tin 2 (reset context)
T3: LIVA stream response cho tin 2
```

---

## 🔴 Vấn Đề 2: Terminal Báo Lỗi Sập Engine (ECONNRESET → ECONNREFUSED)

### 📌 Mô Tả Lỗi

Python Engine (port 8100) bị chết khi user chat 2 lần liên tiếp. Log cho thấy chuỗi lỗi:
1. `ECONNRESET` — Kết nối TCP bị ngắt
2. `ECONNREFUSED` — Gateway thử reconnect nhưng Server đã chết hoàn toàn

---

### 🔬 Phân Tích Nguyên Nhân Gốc Rễ

#### 2 Kịch Bản Song Song

#### **Kịch Bản 1: VRAMGuard Tự Bóp Cò**

```
T0: LIVA đang suy nghĩ (Generate Token) → GPU 100%
T1: VRAMGuard.#tick() chạy (chu kỳ 10s)
T2: Layer 2: #queryGpuUtilization() → trả về > 75%
T3: VRAMGuard nghĩ: "User đang chơi game!" → gọi killLlamaServer()
T4: Python Engine bị kill → ECONNREFUSED
```

**Root Cause:** VRAMGuard không kiểm tra `AgentLoop.isBusy` trước khi đánh giá GPU utilization.

#### VRAMGuard.ts (Dòng 156-164) — Điểm Lỗi
```startLine:156:endLine:164:openclaw-gateway\src\services\VRAMGuard.ts
            // --- Layer 2: nvidia-smi GPU Utilization (only if no whitelist match) ---
            if (!heavyApp && !this.#isYielded) {
                const gpuUtil = await this.#queryGpuUtilization();
                if (gpuUtil !== null && gpuUtil > GPU_UTIL_THRESHOLD) {
                    this.#isYielded = true;
                    logger.warn(`[VRAMGuard] ⚡ GPU utilization ${gpuUtil}% > ${GPU_UTIL_THRESHOLD}% — yielding VRAM!`);
                    this.emit("yield_vram", { reason: `GPU util ${gpuUtil}%`, gpuUtil });
                }
            }
```

**Vấn đề:** Khi AI đang generate token, GPU utilization tự nhiên cao (>75%). VRAMGuard nhầm lẫn đây là game/app nặng.

#### **Kịch Bản 2: AgentLoop Tự Sát**

```
T0: Tin nhắn 2 bị chặn (isBusy = true)
T1: MemoryManager cố lưu log vào Vector DB
T2: Gọi EmbeddingService.embedWithTimeout()
T3: Lỗi kết nối (do VRAMGuard kill server)
T4: AgentLoop hoảng loạn → gọi restartRouter()
T5: restartRouter() → gọi wmic process call terminate → kill Python Engine
```

#### AgentLoop.ts (Dòng 662-666) — Điểm Lỗi
```startLine:662:endLine:666:openclaw-gateway\src\core\AgentLoop.ts
                    if (isNetworkError) {
                        logger.error("🛑 Mất kết nối HTTP tới llama-server (AI Core). Đang tự phục hồi...");
                        this.#orchestrator.startAnomalyDetection();
                        this.#orchestrator.restartRouter(); // Tái khởi động (Rewarm)
                    }
```

**Vấn đề:** Khi VRAMGuard yield (vì phát hiện "game"), lỗi mạng là **expected behavior**, không phải anomaly. AgentLoop không phân biệt được 2 trường hợp:
- Lỗi mạng thật (server crash)
- VRAMGuard yield (server bị kill có chủ đích)

---

### 📂 Code Liên Quan

#### VRAMGuard.ts — Layer 2 GPU Check
```startLine:44:endLine:52:openclaw-gateway\src\services\VRAMGuard.ts
/** Minimum GPU utilization % to trigger yield (nvidia-smi layer) */
const GPU_UTIL_THRESHOLD = 75;

/** Minimum free VRAM (MB) before yielding — if more than this is free, coexist peacefully */
const VRAM_SAFETY_MB = 1500; // 1.5GB safety buffer

/** Polling interval in ms (default 10s — low overhead) */
const DEFAULT_POLL_MS = 10_000;
```

#### VRAMGuard.ts — Yield Event Handler (CoreKernel.ts)
```startLine:1206:endLine:1216:openclaw-gateway\src\core\CoreKernel.ts
    this.vramGuard.on("yield_vram", async (payload: { reason: string; appName?: string }) => {
        logger.warn(`[v24 VRAMGuard] 🎮 YIELDING VRAM: ${payload.reason}`);
        this.addTelemetryLog("warn", `VRAM Yielded: ${payload.reason}`);
        this.ui.broadcastUIEvent("system_notification", {
            title: "🎮 VRAM Yielded",
            body: `LIVA đã nhường GPU cho ${payload.appName || "ứng dụng nặng"}. Chuyển sang Cloud AI.`,
            type: "info"
        });
        // Kill local LLM to free VRAM
        await this.agentLoop.Orchestrator.killLlamaServer();
    });
```

---

### 🛠️ Đề Xuất Khắc Phục

#### **Với VRAMGuard: Thêm IS_Busy Check**

**Thay đổi cần thiết:**

1. **VRAMGuard.ts** — Inject `isAgentBusy` checker:
```startLine:58:endLine:98:openclaw-gateway\src\services\VRAMGuard.ts
export class VRAMGuard extends EventEmitter {
    #pollTimer: NodeJS.Timeout | null = null;
    #isYielded = false;
    #lastHeavyApp: string | null = null;
    #coexistLogged = false;
    #pollIntervalMs: number;
    #enabled = true;
    
    // [FIX] Inject AgentLoop busy checker
    #isAgentBusy: () => boolean = () => false;

    constructor(pollIntervalMs: number = DEFAULT_POLL_MS) {
        super();
        this.#pollIntervalMs = pollIntervalMs;
    }
    
    // [FIX] Setter để CoreKernel inject checker
    public setAgentBusyChecker(checker: () => boolean): void {
        this.#isAgentBusy = checker;
    }
```

2. **VRAMGuard.ts** — Skip Layer 2 khi Agent đang busy:
```startLine:156:endLine:174:openclaw-gateway\src\services\VRAMGuard.ts
            // --- Layer 2: nvidia-smi GPU Utilization (only if no whitelist match) ---
            // [FIX] Skip GPU util check when AI is actively generating
            // High GPU % during token generation is NORMAL, not a game
            if (!heavyApp && !this.#isYielded && !this.#isAgentBusy()) {
                const gpuUtil = await this.#queryGpuUtilization();
                if (gpuUtil !== null && gpuUtil > GPU_UTIL_THRESHOLD) {
                    this.#isYielded = true;
                    logger.warn(`[VRAMGuard] ⚡ GPU utilization ${gpuUtil}% > ${GPU_UTIL_THRESHOLD}% — yielding VRAM!`);
                    this.emit("yield_vram", { reason: `GPU util ${gpuUtil}%`, gpuUtil });
                }
            }
```

3. **CoreKernel.ts** — Inject checker khi bootstrap:
```startLine:1198:endLine:1202:openclaw-gateway\src\core\CoreKernel.ts
    // [v24] Pillar 1: Start VRAM Guard + Event Wiring
    await this.vramGuard.loadCustomApps();
    this.vramGuard.start();
    // [v24 FIX] Inject AgentLoop busy checker to prevent false positive
    this.vramGuard.setAgentBusyChecker(() => this.agentLoop.isBusy);
```

#### **Với AgentLoop: Phân Biệt VRAM Yield vs Lỗi Mạng**

**Thay đổi cần thiết:**

```startLine:643:endLine:668:openclaw-gateway\src\core\AgentLoop.ts
                } catch (error: unknown) {
                const errMsg = error instanceof Error ? error.message : String(error);
                    logger.error("Lỗi kết nối Ghost Server:" + " " + errMsg);
                    if (this.onThinkingEnd) this.onThinkingEnd();

                    const isNetworkError = errMsg.includes("ECONNREFUSED") || errMsg.includes("fetch failed") || errMsg.includes("timeout") || errMsg.includes("AbortError") || errMsg.includes("14 UNAVAILABLE");
                    const isVramYielded = errMsg.includes("VRAM yielded") || errMsg.includes("embedding unavailable");

                    // [v25 FIX] VRAMGuard mid-request: GPU was yielded to user's game/app
                    // Give a friendly response instead of raw error. Do NOT restart router —
                    // VRAMGuard lifecycle handles kill/respawn separately.
                    if (isVramYielded) {
                        logger.warn("[AgentLoop] VRAM was yielded mid-request. Responding gracefully.");
                        if (this.onSpokenResponse) {
                            this.onSpokenResponse("Anh ơi, em vừa nhường GPU cho game của anh rồi nên tạm thời không xử lý được. Khi nào tắt game, em sẽ tự động quay lại phục vụ nhé!");
                        }
                        return; // [FIX] KHÔNG gọi restartRouter() khi VRAM yielded
                    }

                    if (isNetworkError) {
                        // [FIX] Kiểm tra xem VRAMGuard có đang yield không
                        // Nếu VRAMGuard đang yield, đây là expected behavior, không restart
                        const vramGuard = (this.#orchestrator as any).vramGuard;
                        if (vramGuard?.isYielded) {
                            logger.info("[AgentLoop] VRAMGuard đang yield — bỏ qua restartRouter()");
                            if (this.onSpokenResponse) {
                                this.onSpokenResponse("GPU đang được sử dụng bởi ứng dụng khác. Em sẽ quay lại ngay khi có thể.");
                            }
                            return;
                        }
                        logger.error("🛑 Mất kết nối HTTP tới llama-server (AI Core). Đang tự phục hồi...");
                        this.#orchestrator.startAnomalyDetection();
                        this.#orchestrator.restartRouter(); // Tái khởi động (Rewarm)
                    }
```

---

## 📊 Tổng Kết Đề Xuất

| Vấn Đề | Độ Ưu Tiên | Độ Phức Tạp | Cách Khắc Phục |
|---------|-------------|--------------|-----------------|
| Chat 2 lần liên tiếp | **HIGH** | Trung bình | Cách A (Toast) hoặc Cách B (Abort + Toast) |
| VRAMGuard tự bóp cò | **CRITICAL** | Thấp | Thêm `isAgentBusy` check + Phân biệt yield vs lỗi mạng |

---

## 📝 Checklist Trước Khi Fix

- [ ] Chạy `gitnexus_detect_changes()` để xác nhận chỉ thay đổi các file cần thiết
- [ ] Kiểm tra tất cả unit test liên quan
- [ ] Test với scenario: chat 2 lần liên tiếp
- [ ] Test với scenario: chạy game nặng trong khi LIVA đang suy nghĩ

---

## 🔗 Related Files

| File | Mô Tả |
|------|--------|
| `openclaw-gateway/src/core/AgentLoop.ts` | Main FSM: IDLE→THINKING→ACTING→REFLECTING |
| `openclaw-gateway/src/services/VRAMGuard.ts` | GPU monitoring và VRAM yielding |
| `openclaw-gateway/src/core/CoreKernel.ts` | Kernel bootstrap và event wiring |
| `openclaw-gateway/src/core/events/ReactiveSync.ts` | AgentLoop lifecycle callbacks |
| `liva-ui/src/components/VoiceChat.vue` | Chat UI component |
| `openclaw-gateway/src/core/UIController.ts` | WebSocket bridge |

---

*Báo cáo này được tạo bởi Claude AI Research Agent — 2026-05-14*
