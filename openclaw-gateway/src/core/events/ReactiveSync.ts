import { logger } from "../../utils/logger";
import type { AgentLoop } from "../AgentLoop";
import type { UIController } from "../UIController";
import type { IVoiceEngine } from "../../services/IVoiceEngine";
import type { WhisperNode } from "../../services/WhisperNode";

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
    whisperNode: WhisperNode;
    dispatch: (id: string, payload: any) => Promise<void>;
    addTelemetryLog: (level: string, message: string) => void;
    isTtsFallbackActive: () => boolean;
    setTtsFallbackActive: (active: boolean) => void;
    createFallbackVoiceEngine: () => IVoiceEngine;
    onFallbackVoiceEngineCreated: (engine: IVoiceEngine) => void;
}

export function wireReactiveSync(deps: ReactiveSyncDeps): void {
    const {
        agentLoop, ui, getVoiceEngine, setVoiceEngine, whisperNode,
        dispatch, addTelemetryLog, isTtsFallbackActive, setTtsFallbackActive,
        createFallbackVoiceEngine, onFallbackVoiceEngineCreated,
    } = deps;

    // --- THINKING LIFECYCLE ---
    agentLoop.onThinkingStart = async () => {
        getVoiceEngine()?.preempt();
        whisperNode.flush();
        await dispatch("ui_broadcast", { name: "ai_thinking_start" });
    };

    agentLoop.onThinkingEnd = async () => {
        await dispatch("ui_broadcast", { name: "ai_thinking_end" });
    };

    // --- SPOKEN RESPONSE (with HEARTBEAT_OK suppression) ---
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

    // --- STREAM START (TTS Circuit Breaker) ---
    agentLoop.onStreamStart = async () => {
        // 🩺 [Circuit Breaker] Health check TTS once before stream
        const voiceEngine = getVoiceEngine();
        if (voiceEngine && !isTtsFallbackActive()) {
            const isAlive = await voiceEngine.speak(" ");
            if (isAlive === false) {
                logger.error({ context: "CoreKernel" }, "Tiến trình Python Edge-TTS mất kết nối. Kích hoạt Fallback sang Kokoro Local...");
                await voiceEngine.destroy();
                const fallback = createFallbackVoiceEngine();
                setVoiceEngine(fallback);
                setTtsFallbackActive(true);
                onFallbackVoiceEngineCreated(fallback);
            }
        }
        await dispatch("ui_broadcast", { name: "ai_stream_start" });
    };

    // --- STREAM CHUNK (voice + UI, fire-and-forget) ---
    // ⚡ [PERF] Fire-and-forget dispatch — KHÔNG await để tránh back-pressure block gRPC stream
    agentLoop.onStreamChunk = async (chunk: string) => {
        if (chunk.includes("HEARTBEAT_OK")) return;

        getVoiceEngine()?.pushTokens(chunk);

        // ⚡ [PERF] Fire-and-forget — KHÔNG await để tránh back-pressure block gRPC/HTTP stream
        dispatch("ui_broadcast", {
            name: "ai_stream_chunk",
            data: { textChunk: chunk }
        }).catch(e => logger.error(`[Stream] Broadcast error: ${e}`));
    };

    // --- [v23 PILLAR 3] LATENCY MASKING (filler audio for heavy routes) ---
    // Plays a short pre-recorded filler ("Dạ...", "Hmm...") while LLM loads VRAM.
    // Perceived latency = 0ms. Actual TTFT = 1.5-3s hidden behind filler.
    agentLoop.onLatencyMask = (route: string) => {
        const fillerMap: Record<string, string> = {
            deep_reasoning: "Hmm, để em suy nghĩ chút...",
            tool_execution: "Dạ vâng, đợi em xử lý...",
        };
        const filler = fillerMap[route] || "Dạ...";
        logger.debug(`[v23 Latency Mask] 🎭 Playing filler for route: ${route}`);

        // Emit filler text to TTS (instant speech while LLM warms VRAM)
        getVoiceEngine()?.pushTokens(filler + ".");

        // Also notify UI
        dispatch("ui_broadcast", {
            name: "ai_filler_response",
            data: { text: filler, route }
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

            const handler = (payload: any) => {
/* istanbul ignore next */
                if (payload.approvalId === approvalId) {
                    clearTimeout(timeout);
                    ui.removeListener("exec_approval_response", handler);
                    resolve({
                        approved: payload.approved === true,
                        editedCommand: payload.editedCommand
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
