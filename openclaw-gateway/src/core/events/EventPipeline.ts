/**
 * EventPipeline — Sprint 3 Task 3.1
 *
 * Extracted from CoreKernel constructor (Lines 380-619).
 * Wires internal UI, Audio, Camera, Dashboard, and ReactiveSync events.
 *
 * IMPORTANT: This class does NOT own the services.
 * CoreKernel still holds them as public properties for backward-compat.
 * EventPipeline only *wires the event listeners* between them.
 */

import type { DependencyContainer } from "../DependencyContainer";
import { logger } from "../../utils/logger";

export class EventPipeline {
    #deps: DependencyContainer;

    constructor(deps: DependencyContainer) {
        this.#deps = deps;
    }

    /**
     * Wire all internal event listeners.
     * Called once from CoreKernel constructor after all services are instantiated.
     */
    wireListeners(): void {
        this.#wireAudioPipeline();
        this.#wireZMASEvents();
        this.#wireDashboardEvents();
        // NOTE: camera_frame is handled directly by CoreKernel (true-private #latestCameraFrame)
        this.#setupReactiveSync();
    }

    // --- AUDIO PIPELINE (ZERO-LATENCY) ---
    #wireAudioPipeline(): void {
        const { ui, whisperNode, voiceEngine, vadBridge } = this.#deps;

        /**
         * [v22 Sentient Omni-Duplex Pipeline]
         * Audio flow (correct):
         *   Frontend WASM VAD → Float32 PCM chunks
         *   → VADWorkerBridge.processAudio() — accumulate in ring buffer
         *   → emit("speech_end") when silence detected
         *   → WhisperNode.triggerTranscription() — SINGLE transcription per utterance
         *
         * ANTI-PATTERN (fixed): Direct WhisperNode.pushAudioChunk() caused every chunk
         * to trigger transcription via silence timer → DDoS on Whisper port 8100.
         */
        if (vadBridge) {
            // PRIMARY PATH: Use VADWorkerBridge for neural VAD detection
            ui.on("audio_input", (buffer: Buffer) => {
                // Convert Buffer to Float32Array for VADWorker
                const float32 = new Float32Array(buffer.buffer, buffer.byteOffset, buffer.byteLength / 4);
                vadBridge.pushAudioSamples(float32);
            });

            // Wire VAD events → Whisper transcription (ONLY on speech_end)
            vadBridge.on("speech_start", () => {
                logger.debug("[VAD] 🎙️ SPEECH_START — start audio accumulation");
            });

            vadBridge.on("speech_end", () => {
                logger.debug("[VAD] 🔇 SPEECH_END — triggering single transcription");
                whisperNode.triggerTranscription();
            });
        } else {
            // FALLBACK PATH: Legacy silence-timer VAD (not recommended — causes spam)
            logger.warn("[AudioPipeline] ⚠️ VADWorkerBridge not initialized. Using legacy silence timer (may cause transcription spam).");
            ui.on("audio_input", (buffer: Buffer) => {
                whisperNode.pushAudioChunk(buffer);
            });
        }

        ui.on("interrupt", () => {
            logger.warn(`[CoreKernel] 🛑 Nhận lệnh NGẮT từ UI. Dừng TTS và xóa buffer.`);
            voiceEngine.preempt();
            whisperNode.flush();
        });

        whisperNode.on("transcription_ready", async (text: string) => {
            // [P5] Stop mic immediately to prevent feedback loop
            // (AI response → speaker → mic → unwanted STT like "Dạ, em")
            ui.broadcastUIEvent("mic_stop", {});

            // [P5] Sanitize STT feedback contamination
            // Strip trailing AI personality fragments that mic picked up
            let sanitized = text
                .replace(/[,\s]*(Dạ|dạ|Em|em|Ạ|ạ)[,\s]*$/gi, '')
                .trim();

            // Additional pass for combinations at beginning or end
            sanitized = sanitized
                .replace(/^(Dạ[,\s]+em|Dạ)[,\s]+/gi, '')
                .replace(/[,\s]+(Dạ[,\s]+em|Dạ|ạ|em|nhé|nha|ạ)[,\s]*$/gi, '')
                .trim();

            if (!sanitized) return; // Skip empty after sanitization

            await this.#deps.dispatch("agent_input", sanitized);
        });

        voiceEngine.on("audio_base64", (base64: string) => {
            ui.broadcastUIEvent("ai_audio_chunk", { audio: base64 });
        });
    }

    // --- Z-MAS EVENT PIPELINE ---
    #wireZMASEvents(): void {
        const { agentLoop, voiceEngine, whisperNode } = this.#deps;

        agentLoop.Orchestrator.on("suspend_peripherals", () => {
            logger.warn(`[Z-MAS] 🛑 Singularit Mode! Đóng băng Thanh quản và Mắt để tối ưu 100% VRAM cho 26B!`);
            voiceEngine.preempt();
            whisperNode.flush();
        });

        agentLoop.Orchestrator.on("resume_peripherals", () => {
            logger.info(`[Z-MAS] 🟢 Expert đã xả VRAM. Kích hoạt lại Thanh quản và Lỗ tai...`);
        });
    }

    // --- DASHBOARD EVENT HANDLERS (Multi-Window Support) ---
    #wireDashboardEvents(): void {
        const { ui, registry } = this.#deps;

        ui.on("get_skills_list", (ws: any) => {
            const whitelistData = registry.whitelist.getAll();
            const openCircuits = registry.circuitBreaker.getOpenCircuits();
            const skills = registry.getAllSkills().map((s: { name: string; description: string; isCoreSkill?: boolean; category?: string }) => {
                const isOpen = openCircuits.has(s.name);
                const wlEntry = whitelistData[s.name];
                const isEnabled = wlEntry ? wlEntry.enabled : true;
                return {
                    name: s.name,
                    description: s.description,
/* istanbul ignore next */
                    isCoreSkill: s.isCoreSkill || false,
                    category: s.category || (s.isCoreSkill ? "Core" : "Extension"),
                    status: !isEnabled ? "disabled" : isOpen ? "error" : "active",
                    enabled: isEnabled,
                    errorMsg: isOpen ? registry.circuitBreaker.getCircuitError(s.name) : null,
                };
            });
            ui.sendSkillsList(ws, skills);
        });

        ui.on("get_system_status", (ws: any) => {
            const status = {
                model: process.env.EXPERT_MODEL_NAME || "Unknown",
                provider: process.env.AI_PROVIDER || "local",
                uptime: process.uptime(),
                memoryUsage: process.memoryUsage().heapUsed,
                telemetry: [] as unknown[], // Telemetry is managed by CoreKernel directly
            };
            ui.sendSystemStatus(ws, status);
        });

        // [P5] Memory Reset — Dashboard triggers full memory wipe
        ui.on("reset_memory", async (ws: any) => {
            logger.warn("[EventPipeline] 🧹 Nhận lệnh RESET MEMORY từ Dashboard!");
            const result = await this.#deps.memory.resetAllMemory();
            if (ws.readyState === 1) { // WebSocket.OPEN
                ws.send(JSON.stringify({
                    event: "memory_reset_result",
                    payload: result,
                }));
            }
            if (result.success) {
                ui.broadcastUIEvent("memory_reset_complete", {});
            }
        });
    }

    // --- CAMERA VISION (Webcam → AI Multimodal) ---
    #wireCameraVision(): void {
        const { ui } = this.#deps;

        ui.on("camera_frame", (payload: { image: string; timestamp: number }) => {
            // Camera frame is stored at CoreKernel level (true private #latestCameraFrame)
            // We just log here; CoreKernel's own listener stores the actual frame
            logger.info(`[Camera] 📸 Nhận frame webcam (${Math.round(payload.image.length / 1024)}KB)`);
        });
    }

    // --- REACTIVE SYNC (AgentLoop callbacks → UI + Voice) ---
    #setupReactiveSync(): void {
        const { agentLoop, voiceEngine, whisperNode, ui } = this.#deps;

        agentLoop.onThinkingStart = async () => {
            voiceEngine.preempt();
            whisperNode.flush();
            await this.#deps.dispatch("ui_broadcast", { name: "ai_thinking_start" });
        };

        agentLoop.onThinkingEnd = async () => {
            await this.#deps.dispatch("ui_broadcast", { name: "ai_thinking_end" });
        };

        agentLoop.onSpokenResponse = async (text: string) => {
            // Bắt và triệt tiêu chuỗi HEARTBEAT_OK
            if (text.trim() === "HEARTBEAT_OK" || text.includes("HEARTBEAT_OK")) {
                logger.info(`[Heartbeat] 🤫 Nhịp đập ổn định. Đã triệt tiêu âm thanh.`);
                return;
            }
            // [P5] Flush TTSFormatter buffer — gửi nốt câu cuối còn sót trong bộ đệm
            voiceEngine.flushTTS();
            await this.#deps.dispatch("ui_broadcast", {
                name: "ai_spoken_response",
                data: { text }
            });
        };

        agentLoop.onStreamStart = async () => {
            await this.#deps.dispatch("ui_broadcast", { name: "ai_stream_start" });
        };

        // Gộp voiceEngine.pushTokens + UI broadcast vào 1 handler duy nhất
        // (trước đây bị gán 2 lần, handler sau override handler đầu → TTS bị câm)
        agentLoop.onStreamChunk = async (chunk: string) => {
            if (chunk.includes("HEARTBEAT_OK")) return;
            voiceEngine.pushTokens(chunk); // TTS feed
            await this.#deps.dispatch("ui_broadcast", {
                name: "ai_stream_chunk",
                data: { textChunk: chunk }
            });
        };

        // [Z-MAS ZERO-TRUST] Exec Approval Wiring
        agentLoop.onExecApprovalRequired = (toolName, command, reason) => {
            return new Promise((resolve) => {
                const approvalId = Date.now().toString() + Math.random().toString(36).substring(7); // NOSONAR

                // Timeout 30s: Tự động từ chối nếu không có phản hồi
                const timeout = setTimeout(() => {
                    ui.removeListener("exec_approval_response", handler);
                    logger.warn(`[Zero-Trust] Quá thời gian 30s. Tự động TỪ CHỐI lệnh: ${toolName}`);
                    resolve({ approved: false });
                }, 30000);

                const handler = (payload: { approvalId: string; approved: boolean; editedCommand?: string }) => {
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
                this.#deps.dispatch("ui_broadcast", {
                    name: "exec_approval_required",
                    data: { approvalId, toolName, command, reason }
                }).catch(e => {
                    logger.error(`[Zero-Trust] Lỗi khi gửi broadcast phê duyệt:`, e);
                });
            });
        };

        // --- [DevSecOps] AI Self-Healing UI Events ---
        agentLoop.Orchestrator.on("anomaly_detected", () => {
            logger.warn("[CoreKernel] ⚠️ Đã nhận tín hiệu Anomaly từ Orchestrator. Chuẩn bị tự phục hồi...");
            this.#deps.addTelemetryLog('error', 'AI Zombie Process Anomaly Detected (Self-healing triggered)');
        });

        agentLoop.Orchestrator.on("rewarming_ai", async () => {
            this.#deps.addTelemetryLog('warning', 'Rewarming AI (Re-allocating VRAM)');
            await this.#deps.dispatch("ui_broadcast", {
                name: "system_notification",
                data: { message: "⚡ LIVA đang tái cấu trúc bộ nhớ đồ họa (Rewarming AI)...", freezeUI: true }
            });
        });

        agentLoop.Orchestrator.on("rewarming_complete", async () => {
            this.#deps.addTelemetryLog('info', 'AI Rewarming Complete');
            await this.#deps.dispatch("ui_broadcast", {
                name: "system_notification",
                data: { message: "✅ Bộ nhớ đồ họa đã ổn định. LIVA đã sẵn sàng!", freezeUI: false }
            });
        });
    }
}
