import { logger } from "../../utils/logger";
import type { AgentLoop } from "../AgentLoop";
import type { UIController } from "../UIController";
import type { IVoiceEngine } from "../../services/IVoiceEngine";
import type { NemotronSTTService } from "../../services/NemotronSTTService";
import type { TelegramBridge } from "../../channels/TelegramBridge";

/**
 * ReactiveSync — Wires AgentLoop lifecycle callbacks to CoreKernel subsystems.
 *
 * Extracted from CoreKernel.#setupReactiveSync() (112 LOC) to reduce the
 * CoreKernel constructor complexity and enable independent testing.
 *
 * This module handles:
 *   1. Thinking start/end → voice preemption + UI broadcast
 *   2. Stream start → TTS circuit breaker health check
 *   3. Stream chunk → TTS token push + UI broadcast (fire-and-forget)
 *   4. Spoken response → HEARTBEAT_OK suppression + UI broadcast
 *   5. Exec approval → 30s timeout + UI prompt + approval resolution
 *   6. Z-MAS anomaly/rewarming events → telemetry + UI notification
 */

export interface ReactiveSyncDeps {
    agentLoop: AgentLoop;
    ui: UIController;
    getVoiceEngine: () => IVoiceEngine | null;
    setVoiceEngine: (engine: IVoiceEngine) => void;
    whisperNode: NemotronSTTService;
    dispatch: (id: string, payload: unknown) => Promise<void>;
    addTelemetryLog: (level: string, message: string) => void;
    isTtsFallbackActive: () => boolean;
    setTtsFallbackActive: (active: boolean) => void;
    createFallbackVoiceEngine: () => IVoiceEngine;
    onFallbackVoiceEngineCreated: (engine: IVoiceEngine) => void;
    getPresence: () => "ACTIVE" | "AWAY";
    getOwnerTelegramId: () => string;
    telegramBridge: TelegramBridge;
}

export function wireReactiveSync(deps: ReactiveSyncDeps): void {
    const {
        agentLoop, ui, getVoiceEngine, setVoiceEngine, whisperNode,
        dispatch, addTelemetryLog, isTtsFallbackActive, setTtsFallbackActive,
        createFallbackVoiceEngine, onFallbackVoiceEngineCreated,
        getPresence, getOwnerTelegramId, telegramBridge,
    } = deps;

    // --- THINKING LIFECYCLE ---
    agentLoop.onThinkingStart = async () => {
        if (getPresence() !== "AWAY") {
            getVoiceEngine()?.preempt();
            whisperNode.flush();
            await dispatch("ui_broadcast", { name: "ai_thinking_start" });
        }
    };

    agentLoop.onThinkingEnd = async () => {
        if (getPresence() !== "AWAY") {
            await dispatch("ui_broadcast", { name: "ai_thinking_end" });
        }
    };

    // --- SPOKEN RESPONSE (with HEARTBEAT_OK suppression) ---
    agentLoop.onSpokenResponse = async (text: string) => {
        if (text.trim() === "HEARTBEAT_OK" || text.includes("HEARTBEAT_OK")) {
            logger.info(`[Heartbeat] 🤫 Nhịp đập ổn định. Đã triệt tiêu âm thanh.`);
            return;
        }
        if (getPresence() === "AWAY") {
            const ownerId = getOwnerTelegramId();
            if (ownerId) {
                logger.info(`[Presence] Rerouting response to Telegram: "${text.substring(0, 50)}..."`);
                await telegramBridge.sendText(ownerId, text);
            }
            return;
        }
        // [P5] Flush TTSFormatter buffer — gửi nốt câu cuối còn sót trong bộ đệm
        getVoiceEngine()?.flushTTS();
        await dispatch("ui_broadcast", {
            name: "ai_spoken_response",
            data: { text }
        });
    };

    // [v25 FIX] SYSTEM BUSY NOTIFICATION
    // When user sends a 2nd message while AI is generating, show a toast instead of chat bubble
    agentLoop.onSystemBusy = async (message: string) => {
        if (getPresence() !== "AWAY") {
            await dispatch("ui_broadcast", {
                name: "system_busy",
                data: { message }
            });
        }
    };

    // --- STREAM START (TTS Circuit Breaker) ---
    // ⚡ [PERF C9] Fire-and-forget health probe — KHÔNG block stream start
    // Old: await voiceEngine.speak(" ") blocked 50-3000ms before ui_broadcast
    agentLoop.onStreamStart = async () => {
        if (getPresence() === "AWAY") return;
        // 🩺 [Circuit Breaker] Health check TTS in background (non-blocking)
        const voiceEngine = getVoiceEngine();
        if (voiceEngine && !isTtsFallbackActive()) {
            voiceEngine.speak(" ").then(isAlive => {
                if (isAlive === false) {
                    logger.error({ context: "CoreKernel" }, "Tiến trình Python Edge-TTS mất kết nối. Kích hoạt Fallback sang Kokoro Local...");
                    voiceEngine.destroy().then(() => {
                        const fallback = createFallbackVoiceEngine();
                        setVoiceEngine(fallback);
                        setTtsFallbackActive(true);
                        onFallbackVoiceEngineCreated(fallback);
                    }).catch(e => logger.error({ err: e }, "[TTS Fallback] Destroy error"));
                }
            }).catch(e => logger.warn(`[TTS Health] Probe failed: ${e}`));
        }
        await dispatch("ui_broadcast", { name: "ai_stream_start" });
    };

    // --- STREAM CHUNK (voice + UI, fire-and-forget) ---
    // ⚡ [PERF] Fire-and-forget dispatch — KHÔNG await để tránh back-pressure block gRPC stream
    agentLoop.onStreamChunk = async (chunk: string) => {
        if (chunk.includes("HEARTBEAT_OK")) return;
        if (getPresence() === "AWAY") return;

        getVoiceEngine()?.pushTokens(chunk);

        // ⚡ [PERF] Fire-and-forget — KHÔNG await để tránh back-pressure block gRPC/HTTP stream
        dispatch("ui_broadcast", {
            name: "ai_stream_chunk",
            data: { textChunk: chunk }
        }).catch(e => logger.error(`[Stream] Broadcast error: ${e}`));
    };

    agentLoop.onThoughtChunk = async (chunk: string) => {
        if (getPresence() === "AWAY") return;
        dispatch("ui_broadcast", {
            name: "ai_stream_chunk",
            data: { textChunk: chunk, isThought: true }
        }).catch(e => logger.error(`[Stream] Broadcast error: ${e}`));
    };

    agentLoop.onRecoveryReset = async () => {
        if (getPresence() === "AWAY") return;
        await dispatch("ui_broadcast", {
            name: "ai_stream_reset"
        });
    };

    // --- [v23 PILLAR 3] LATENCY MASKING (filler audio for heavy routes) ---
    // ⚡ [PERF P0-D] Filler audio now bypasses TTS synthesis entirely.
    // Frontend plays pre-recorded .wav files locally (0ms delay).
    // OLD: pushTokens(filler) → Python TTS synthesis (200-500ms) → audio chunk → play
    // NEW: broadcast event → Frontend plays cached AudioBuffer instantly
    agentLoop.onLatencyMask = (route: string) => {
        if (getPresence() === "AWAY") return;
        logger.debug(`[v23 Latency Mask] 🎭 Emitting filler event for route: ${route}`);

        // Notify UI to play pre-recorded filler audio (bypasses TTS pipeline)
        dispatch("ui_broadcast", {
            name: "ai_filler_response",
            data: { route, fillerType: "thinking" }
        }).catch(e => logger.error(`[Latency Mask] Broadcast error: ${e}`));
    };

    // --- EXEC APPROVAL (30s timeout + UI prompt) ---
    agentLoop.onExecApprovalRequired = (toolName, command, reason) => {
        return new Promise((resolve) => {
            const approvalId = Date.now().toString() + Math.random().toString(36).substring(7); // NOSONAR

            // Timeout 30s: Tự động từ chối nếu không có phản hồi
            const timeout = setTimeout(() => {
                ui.removeListener("exec_approval_response", handler);
                logger.warn(`[Zero-Trust] Quá thời gian 30s. Tự động TỪ CHỐI lệnh: ${toolName}`);
                resolve({ approved: false });
            }, 30000);

            const handler = (payload: unknown) => {
/* istanbul ignore next */
                const data = payload as { approvalId?: string; approved?: boolean; editedCommand?: string };
                if (data.approvalId === approvalId) {
                    clearTimeout(timeout);
                    ui.removeListener("exec_approval_response", handler);
                    resolve({
                        approved: data.approved === true,
                        editedCommand: data.editedCommand
                    });
                }
            };

            ui.on("exec_approval_response", handler);

            // Phát tín hiệu ra UI
            dispatch("ui_broadcast", {
                name: "exec_approval_required",
                data: { approvalId, toolName, command, reason }
            }).catch(e => {
                logger.error(`[Zero-Trust] Lỗi khi gửi broadcast phê duyệt:`, e);
            });
        });
    };

    // --- Z-MAS SELF-HEALING EVENTS ---
    agentLoop.Orchestrator.on("anomaly_detected", () => {
        logger.warn("[CoreKernel] ⚠️ Đã nhận tín hiệu Anomaly từ Orchestrator. Chuẩn bị tự phục hồi...");
        addTelemetryLog('error', 'AI Zombie Process Anomaly Detected (Self-healing triggered)');
    });

    agentLoop.Orchestrator.on("rewarming_ai", async () => {
        addTelemetryLog('warning', 'Rewarming AI (Re-allocating VRAM)');
        await dispatch("ui_broadcast", {
            name: "system_notification",
            data: { message: "⚡ LIVA đang tái cấu trúc bộ nhớ đồ họa (Rewarming AI)...", freezeUI: true }
        });
    });

    agentLoop.Orchestrator.on("rewarming_complete", async () => {
        addTelemetryLog('info', 'AI Rewarming Complete');
        await dispatch("ui_broadcast", {
            name: "system_notification",
            data: { message: "✅ Bộ nhớ đồ họa đã ổn định. LIVA đã sẵn sàng!", freezeUI: false }
        });
    });
}
