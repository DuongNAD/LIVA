import type { CoreKernel } from "../CoreKernel";
import type { WebSocket } from "ws";
import type { NormalizedMessage } from "../../channels/ChannelNormalizer";
import type { VADWorkerBridge } from "../../services/VADWorkerBridge";
import { logger } from "../../utils/logger";
import { TraceContext } from "../../utils/TraceContext";
import { HITLGuard } from "../../security/HITLGuard";
import { ConfigManager } from "../config/ConfigManager";
import { safeFetch } from "../../utils/HttpClient";
import { memoryEvents } from "../../memory/MemoryEventBus";
import { wireReactiveSync } from "../events/ReactiveSync";
import { KokoroVoiceEngine } from "../../services/KokoroVoiceEngine";
import type { ChatCompletionResponse as NativeIPCChatResponse } from "../../utils/NativeIPCClient";
import LRUCache from "lru-cache";
import OpenAI from "openai";
import * as path from "node:path";
import * as fsp from "node:fs/promises";
import * as os from "node:os";
import { KernelLifecycle } from "./KernelLifecycle";

export class KernelEventRouter {
  private taskPlanHistories = new LRUCache<string, Array<{ role: string; content: string }>>({
    max: 50,
    ttl: 1000 * 60 * 60 * 24, // 24 hours
  });

  constructor(private kernel: CoreKernel) {}

  public registerAll(): void {
    const k = this.kernel;

    memoryEvents.on("NEW_TURN", this.onNewTurnHandler);
    memoryEvents.on("CONSOLIDATION_COMPLETE", this.onConsolidationCompleteHandler);
    memoryEvents.on("rpa_auth_required", this.onRpaAuthRequiredHandler);

    k.powerMonitor.on("battery_mode_changed", this.onBatteryModeChanged);
    k.presenceDetector.on("presence_changed", this.onPresenceChanged);
    k.emailManager.on("email_incoming", this.onEmailIncoming);

    k.ui.on("user_input", this.onUserInput);
    k.ui.on("user_typing", this.onUserTyping);
    k.ui.on("user_typing_cancelled", this.onUserTypingCancelled);
    k.ui.on("get_user_profile", this.onGetUserProfile);
    k.ui.on("update_user_profile", this.onUpdateUserProfile);
    k.ui.on("config_updated", this.onConfigUpdated);

    k.zalo.on("zalo_incoming", this.onZaloIncoming);
    k.telegram.on("message", this.onTelegramMessage);
    k.meta.on("message", this.onMetaMessage);
    k.meta.on("postback", this.onMetaPostback);
    k.telegram.on("callback_query", this.onTelegramCallbackQuery);
    k.cdpBridge.on("approval_required", this.onCdpApprovalRequired);
    k.approvalEngine.on("approval_granted", this.onApprovalGranted);
    k.approvalEngine.on("approval_denied", this.onApprovalDenied);

    k.ui.on("audio_input", this.onAudioInput);
    k.ui.on("wake_word_triggered", this.onWakeWordTriggered);
    k.whisperNode.on("transcription_partial", this.onTranscriptionPartial);
    k.ui.on("interrupt", this.onInterrupt);
    k.ui.on("audio_play_started", this.onAudioPlayStarted);
    k.ui.on("audio_play_finished", this.onAudioPlayFinished);
    k.whisperNode.on("transcription_ready", this.onTranscriptionReady);

    k.agentLoop.Orchestrator.on("suspend_peripherals", this.onSuspendPeripherals);
    k.agentLoop.Orchestrator.on("resume_peripherals", this.onResumePeripherals);

    if (k.voiceEngine) {
      k.voiceEngine.on("audio_buffer", this.onVoiceEngineAudioBuffer);
    }

    k.whisperNode.on("stt_fallback_activated", this.onSttFallbackActivated);
    k.whisperNode.on("stt_fallback_deactivated", this.onSttFallbackDeactivated);
    k.ui.on("web_speech_transcription", this.onWebSpeechTranscription);
    k.ui.on("get_memory_data", this.onGetMemoryData);
    k.ui.on("consolidate_memory", this.onConsolidateMemory);
    k.ui.on("delete_memory_fact", this.onDeleteMemoryFact);
    k.ui.on("get_skills_list", this.onGetSkillsList);
    k.ui.on("test_skill", this.onTestSkill);
    k.ui.on("test_all_skills", this.onTestAllSkills);
    k.ui.on("toggle_skill", this.onToggleSkill);
    k.ui.on("toggle_all_skills", this.onToggleAllSkills);
    k.ui.on("get_tasks", this.onGetTasks);
    k.ui.on("get_ai_config", this.onGetAiConfig);
    k.ui.on("update_ai_config", this.onUpdateAiConfig);
    k.ui.on("test_ai_connection", this.onTestAiConnection);
    k.ui.on("get_voice_status", this.onGetVoiceStatus);
    k.ui.on("get_voice_profiles", this.onGetVoiceProfiles);
    k.ui.on("select_voice_profile", this.onSelectVoiceProfile);
    k.ui.on("start_voice_training", this.onStartVoiceTraining);
    k.ui.on("stop_voice_training", this.onStopVoiceTraining);
    k.ui.on("add_task", this.onAddTask);
    k.ui.on("task_plan_chat", this.onTaskPlanChat);
    k.ui.on("update_task", this.onUpdateTask);
    k.ui.on("delete_task", this.onDeleteTask);
    k.ui.on("execute_task", this.onExecuteTask);
    k.ui.on("force_gc", this.onForceGc);
    k.ui.on("trigger_gitnexus_index", this.onTriggerGitnexusIndex);
    k.ui.on("reload_skills", this.onReloadSkills);
    k.ui.on("get_system_status", this.onGetSystemStatus);
    k.ui.on("reset_memory", this.onResetMemory);
    k.ui.on("camera_frame", this.onCameraFrame);

    this.setupReactiveSync();
  }

  public registerVadBridge(vadBridge: VADWorkerBridge): void {
    vadBridge.on("speech_start", this.onVadSpeechStart);
    vadBridge.on("speech_end", this.onVadSpeechEnd);
  }

  public unregisterAll(): void {
    const k = this.kernel;
    const safeOff = (emitter: unknown, event: string, listener: (...args: never[]) => void) => {
      if (!emitter) return;
      const e = emitter as {
        off?: (event: string, listener: (...args: never[]) => void) => void;
        removeListener?: (event: string, listener: (...args: never[]) => void) => void;
      };
      if (typeof e.off === "function") {
        try {
          e.off(event, listener);
        } catch {
          /* ignore */
        }
      } else if (typeof e.removeListener === "function") {
        try {
          e.removeListener(event, listener);
        } catch {
          /* ignore */
        }
      }
    };

    safeOff(memoryEvents, "NEW_TURN", this.onNewTurnHandler);
    safeOff(memoryEvents, "CONSOLIDATION_COMPLETE", this.onConsolidationCompleteHandler);
    safeOff(memoryEvents, "rpa_auth_required", this.onRpaAuthRequiredHandler);

    safeOff(k.vadBridge, "speech_start", this.onVadSpeechStart);
    safeOff(k.vadBridge, "speech_end", this.onVadSpeechEnd);

    safeOff(k.powerMonitor, "battery_mode_changed", this.onBatteryModeChanged);
    safeOff(k.presenceDetector, "presence_changed", this.onPresenceChanged);
    safeOff(k.emailManager, "email_incoming", this.onEmailIncoming);
    safeOff(k.zalo, "zalo_incoming", this.onZaloIncoming);
    safeOff(k.telegram, "message", this.onTelegramMessage);
    safeOff(k.telegram, "callback_query", this.onTelegramCallbackQuery);
    safeOff(k.meta, "message", this.onMetaMessage);
    safeOff(k.meta, "postback", this.onMetaPostback);
    safeOff(k.cdpBridge, "approval_required", this.onCdpApprovalRequired);
    safeOff(k.approvalEngine, "approval_granted", this.onApprovalGranted);
    safeOff(k.approvalEngine, "approval_denied", this.onApprovalDenied);
    safeOff(k.whisperNode, "transcription_partial", this.onTranscriptionPartial);
    safeOff(k.whisperNode, "transcription_ready", this.onTranscriptionReady);
    safeOff(k.whisperNode, "stt_fallback_activated", this.onSttFallbackActivated);
    safeOff(k.whisperNode, "stt_fallback_deactivated", this.onSttFallbackDeactivated);
    safeOff(k.agentLoop?.Orchestrator, "suspend_peripherals", this.onSuspendPeripherals);
    safeOff(k.agentLoop?.Orchestrator, "resume_peripherals", this.onResumePeripherals);
    safeOff(k.voiceEngine, "audio_buffer", this.onVoiceEngineAudioBuffer);

    safeOff(k.ui, "user_input", this.onUserInput);
    safeOff(k.ui, "user_typing", this.onUserTyping);
    safeOff(k.ui, "user_typing_cancelled", this.onUserTypingCancelled);
    safeOff(k.ui, "get_user_profile", this.onGetUserProfile);
    safeOff(k.ui, "update_user_profile", this.onUpdateUserProfile);
    safeOff(k.ui, "config_updated", this.onConfigUpdated);
    safeOff(k.ui, "audio_input", this.onAudioInput);
    safeOff(k.ui, "wake_word_triggered", this.onWakeWordTriggered);
    safeOff(k.ui, "interrupt", this.onInterrupt);
    safeOff(k.ui, "audio_play_started", this.onAudioPlayStarted);
    safeOff(k.ui, "audio_play_finished", this.onAudioPlayFinished);
    safeOff(k.ui, "web_speech_transcription", this.onWebSpeechTranscription);
    safeOff(k.ui, "get_memory_data", this.onGetMemoryData);
    safeOff(k.ui, "consolidate_memory", this.onConsolidateMemory);
    safeOff(k.ui, "delete_memory_fact", this.onDeleteMemoryFact);
    safeOff(k.ui, "get_skills_list", this.onGetSkillsList);
    safeOff(k.ui, "test_skill", this.onTestSkill);
    safeOff(k.ui, "test_all_skills", this.onTestAllSkills);
    safeOff(k.ui, "toggle_skill", this.onToggleSkill);
    safeOff(k.ui, "toggle_all_skills", this.onToggleAllSkills);
    safeOff(k.ui, "get_tasks", this.onGetTasks);
    safeOff(k.ui, "get_ai_config", this.onGetAiConfig);
    safeOff(k.ui, "update_ai_config", this.onUpdateAiConfig);
    safeOff(k.ui, "test_ai_connection", this.onTestAiConnection);
    safeOff(k.ui, "get_voice_status", this.onGetVoiceStatus);
    safeOff(k.ui, "get_voice_profiles", this.onGetVoiceProfiles);
    safeOff(k.ui, "select_voice_profile", this.onSelectVoiceProfile);
    safeOff(k.ui, "start_voice_training", this.onStartVoiceTraining);
    safeOff(k.ui, "stop_voice_training", this.onStopVoiceTraining);
    safeOff(k.ui, "add_task", this.onAddTask);
    safeOff(k.ui, "task_plan_chat", this.onTaskPlanChat);
    safeOff(k.ui, "update_task", this.onUpdateTask);
    safeOff(k.ui, "delete_task", this.onDeleteTask);
    safeOff(k.ui, "execute_task", this.onExecuteTask);
    safeOff(k.ui, "force_gc", this.onForceGc);
    safeOff(k.ui, "trigger_gitnexus_index", this.onTriggerGitnexusIndex);
    safeOff(k.ui, "reload_skills", this.onReloadSkills);
    safeOff(k.ui, "get_system_status", this.onGetSystemStatus);
    safeOff(k.ui, "reset_memory", this.onResetMemory);
    safeOff(k.ui, "camera_frame", this.onCameraFrame);
  }

  private setupReactiveSync(): void {
    const k = this.kernel;
    wireReactiveSync({
      agentLoop: k.agentLoop,
      ui: k.ui,
      getVoiceEngine: () => k.voiceEngine,
      setVoiceEngine: (engine) => {
        k.voiceEngine = engine;
      },
      whisperNode: k.whisperNode,
      dispatch: (id, payload) => k.dispatch(id, payload),
      addTelemetryLog: (level, message) => k.addTelemetryLog(level, message),
      isTtsFallbackActive: () => k.isTtsFallbackActive,
      setTtsFallbackActive: (active) => {
        k.isTtsFallbackActive = active;
      },
      createFallbackVoiceEngine: () => new KokoroVoiceEngine(),
      onFallbackVoiceEngineCreated: (engine) => {
        engine.on("audio_base64", (base64: string) => {
          k.ui.broadcastUIEvent("ai_audio_chunk", { audio: base64 });
        });
      },
      getPresence: () => k.presence,
      getOwnerTelegramId: () => k.getDefaultRemoteSenderId(),
      telegramBridge: k.telegram,
    });
  }

  private onNewTurnHandler = () => {
    this.kernel.ui.broadcastUIEvent("memory_updated");
  };

  private onConsolidationCompleteHandler = () => {
    this.kernel.ui.broadcastUIEvent("memory_updated");
  };

  private onRpaAuthRequiredHandler = async (payload: { channel: "zalo" | "messenger"; message: string }) => {
    logger.warn(`[CoreKernel] RPA Auth Required Event for ${payload.channel}: ${payload.message}`);
    this.kernel.ui.broadcastUIEvent("rpa_auth_required", payload);
    const ownerTelegramId = this.kernel.getDefaultRemoteSenderId();
    if (ownerTelegramId) {
      const msgText = `🚨 *[CẢNH BÁO RPA LIVA]*: Trình duyệt RPA ${payload.channel.toUpperCase()} yêu cầu xác thực hoặc đăng nhập!\n\n*Chi tiết*: ${payload.message}\n\n_Vui lòng mở trình duyệt trên máy chủ để thao tác._`;
      this.kernel.telegram.sendText(ownerTelegramId, msgText).catch((err: Error) => {
        logger.error(`[CoreKernel] Failed to send Telegram alert: ${err.message}`);
      });
    }
  };

  private onVadSpeechStart = () => {
    logger.debug("[VAD] 🎙️ SPEECH_START — user is speaking → Audio Ducking");
    this.kernel.ui.broadcastUIEvent("audio_ducking", { volume: 0.2 });
  };

  private onVadSpeechEnd = () => {
    logger.debug("[VAD] 🔇 SPEECH_END — triggering Whisper transcription");
    this.kernel.whisperNode.triggerTranscription();
  };

  private onBatteryModeChanged = async (event: { active: boolean }) => {
    if (event.active) {
      await this.kernel.yieldVRAM();
    } else {
      await this.kernel.reclaimVRAM();
    }
  };

  private onPresenceChanged = (event: { presence: "ACTIVE" | "AWAY" }) => {
    this.kernel.presence = event.presence;
    this.kernel.ui.broadcastUIEvent("presence_changed", { presence: event.presence });
  };

  private onEmailIncoming = async (email: { from: string; subject: string; body: string; uid: number | string }) => {
    TraceContext.runWithContext(
      async () => {
        const normalized: NormalizedMessage = {
          channel: "email",
          senderId: email.from,
          senderName: email.from,
          text: `Subject: ${email.subject}\n\n${email.body}`,
          timestamp: Date.now(),
          rawPayload: email,
        };
        await this.kernel.autoReply.handleIncomingMessage(normalized);
      },
      { channel: "email", traceId: `email-incoming-${email.uid}-${Date.now()}` }
    );
  };

  private onUserInput = async (userText: string, isDryRun?: boolean) => {
    const pending = HITLGuard.getPendingByChannel("ui");
    if (pending) {
      const cleanText = userText.trim().toLowerCase();
      if (["yes", "y", "ok", "oke", "okay", "okey", "duyệt", "đồng ý", "approve", "có", "co"].includes(cleanText)) {
        HITLGuard.respond(pending.id, true);
        return;
      } else if (
        ["no", "n", "hủy", "từ chối", "reject", "cancel", "huy", "không", "khong"].includes(cleanText)
      ) {
        HITLGuard.respond(pending.id, false);
        return;
      }
    }

    TraceContext.runWithContext(
      async () => {
        const weight = this.kernel.orchestrationTensor.getWeight(this.kernel.currentLatency);
        await this.kernel.dispatch("agent_input", userText, isDryRun);
        if (weight <= 0.2) {
          logger.warn(`⚠️ [Orchestrator] High latency (${this.kernel.currentLatency}ms). Proceeding anyway.`);
        }
      },
      { channel: "ui", traceId: `ui-${Date.now()}` }
    );
  };

  private onUserTyping = (text: string) => {
    this.kernel.agentLoop.speculativeWarm(text).catch((err) => {
      logger.error(`[CoreKernel] user_typing speculativeWarm error: ${err}`);
    });
  };

  private onUserTypingCancelled = () => {
    this.kernel.agentLoop.clearSpeculativeCache();
  };

  private onGetUserProfile = async (ws: WebSocket) => {
    try {
      const profile = (await this.kernel.memory.getUserProfile()) ?? {};
      this.kernel.ui.sendUserProfile(ws, profile);
    } catch (err) {
      logger.warn(
        `[CoreKernel] get_user_profile failed, sending empty profile: ${err instanceof Error ? err.message : String(err)}`
      );
      this.kernel.ui.sendUserProfile(ws, {});
    }
  };

  private onUpdateUserProfile = async (ws: WebSocket, profileData: Record<string, unknown>) => {
    const name = profileData?.name;
    const birthYear = profileData?.birthYear;
    const nationality = profileData?.nationality;
    if (
      !profileData ||
      typeof name !== "string" ||
      !name.trim() ||
      !String(birthYear || "").trim() ||
      !String(nationality || "").trim()
    ) {
      logger.warn("⚠️ [CoreKernel] Invalid profile update request rejected.");
      if (ws.readyState === 1) {
        ws.send(JSON.stringify({ event: "profile_update_error", payload: { error: "Invalid profile data fields" } }));
      }
      return;
    }

    try {
      await this.kernel.memory.updateUserProfile(profileData);
      const updated = await this.kernel.memory.getUserProfile();

      if (ws.readyState === 1) {
        ws.send(JSON.stringify({ event: "profile_updated_success", payload: updated }));
      }
      this.kernel.ui.broadcastUIEvent("profile_updated_success", updated ?? undefined);

      if (updated && updated.location) {
        const tz = this.kernel.agentLoop.currentSystemTimezone;
        this.kernel.agentLoop.setSystemLocation(updated.location as string, tz);
      }

      if (updated && updated.language) {
        const voice = await this.kernel.loadVoiceConfig();
        const langKey = updated.language;

        const defaultVoices: Record<string, string> = {
          "vi-VN": "vi-VN-HoaiMyNeural",
          "en-US": "en-US-AvaMultilingualNeural",
          "ja-JP": "ja-JP-NanamiNeural",
          "ko-KR": "ko-KR-SunHiNeural",
          "zh-CN": "zh-CN-XiaoxiaoNeural",
        };
        const defaultProfile = defaultVoices[langKey as string] || "vi-VN-HoaiMyNeural";

        const voiceProfiles = this.kernel.getVoiceProfiles();
        const currentProfileObj = voiceProfiles.find((p) => p.id === voice.activeProfile);

        if (!currentProfileObj || currentProfileObj.lang !== langKey) {
          const nextVoice = {
            ...voice,
            language: langKey,
            activeProfile: defaultProfile,
          };
          await this.kernel.persistConfigPatch({ voice: nextVoice });

          this.kernel.ui.broadcastUIEvent("voice_status_updated", { voice: nextVoice });

          if (this.kernel.voiceEngine) {
            (this.kernel.voiceEngine as { setVoiceProfile?: (profile: string) => void }).setVoiceProfile?.(
              defaultProfile
            );
          }
          logger.info(`[CoreKernel] Synced voice config to language ${langKey} and profile ${defaultProfile}`);
        }
      }
    } catch (err: unknown) {
      const errMsg = err instanceof Error ? err.message : String(err);
      logger.error(`❌ [CoreKernel] Lỗi cập nhật user profile: ${errMsg}`);
      if (ws.readyState === 1) {
        ws.send(JSON.stringify({ event: "profile_update_error", payload: { error: errMsg } }));
      }
    }
  };

  private onConfigUpdated = (config: Record<string, unknown> | null | undefined) => {
    KernelLifecycle.handleConfigUpdated(this.kernel, config);
  };

  private onZaloIncoming = async (userText: string, senderId?: string) => {
    if (
      senderId &&
      (!process.env.ZALO_USER_ID ||
        process.env.ZALO_USER_ID.trim() === "" ||
        process.env.ZALO_USER_ID.includes("NHẬP_USER_ID"))
    ) {
      logger.info(`✨ [Zalo Auto-Detect] Phát hiện ZALO_USER_ID mới: ${senderId}. Đang tự động lưu cấu hình...`);
      process.env.ZALO_USER_ID = senderId;

      try {
        const cwd = process.cwd();
        let envPath = path.join(cwd, ".env");
        try {
          await fsp.access(path.join(cwd, "liva-gateway"));
          envPath = path.join(cwd, "liva-gateway", ".env");
        } catch {}

        let envContent = "";
        if (await fsp.access(envPath).then(() => true).catch(() => false)) {
          envContent = await fsp.readFile(envPath, "utf8");
        }

        const regex = /^ZALO_USER_ID=.*$/m;
        if (regex.test(envContent)) {
          envContent = envContent.replace(regex, `ZALO_USER_ID=${senderId}`);
        } else {
          envContent += `\nZALO_USER_ID=${senderId}\n`;
        }

        const tmpEnvPath = `${envPath}.tmp`;
        await fsp.writeFile(tmpEnvPath, envContent, "utf8");
        const { safeRename } = await import("../../utils/FileUtils");
        await safeRename(tmpEnvPath, envPath);
        logger.info(`[Zalo Auto-Detect] ✅ Đã tự động lưu ZALO_USER_ID=${senderId} vào .env`);

        this.kernel.ui.broadcastUIEvent("env_config_updated", {
          key: "ZALO_USER_ID",
          value: senderId,
        });
      } catch (err: unknown) {
        const errMsg = err instanceof Error ? err.message : String(err);
        logger.error(`❌ [Zalo Auto-Detect] Lỗi lưu ZALO_USER_ID: ${errMsg}`);
      }
    }

    const rawText = userText.replace("[Tin nhắn từ Zalo điện thoại]: ", "").trim();

    if (rawText.startsWith("approve:") || rawText.startsWith("reject:")) {
      const parts = rawText.split(":");
      const approved = parts[0] === "approve";
      const approvalId = parts[1];
      logger.info(
        `💬 [Zalo Inbound] Nhận phản hồi phê duyệt từ nút nhấn: ${approved ? "Đồng ý" : "Từ chối"} (ID: ${approvalId})`
      );
      if (approvalId.startsWith("hitl-")) {
        import("../../security/HITLGuard")
          .then((m) => m.HITLGuard.respond(approvalId, approved))
          .catch((e: unknown) => {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[CoreKernel] Failed to load HITLGuard for Zalo approval response: ${errMsg}`);
          });
      } else {
        this.kernel.approvalEngine.resolveApproval(approvalId, approved);
      }
      return;
    }

    const pending = HITLGuard.getPendingByChannel("zalo");
    if (pending) {
      const cleanText = rawText.toLowerCase();
      if (["yes", "y", "ok", "oke", "okay", "okey", "duyệt", "đồng ý", "approve", "có", "co"].includes(cleanText)) {
        HITLGuard.respond(pending.id, true);
        return;
      } else if (
        ["no", "n", "hủy", "từ chối", "reject", "cancel", "huy", "không", "khong"].includes(cleanText)
      ) {
        HITLGuard.respond(pending.id, false);
        return;
      }
    }

    TraceContext.runWithContext(
      async () => {
        const normalized: NormalizedMessage = {
          channel: "zalo",
          senderId: senderId || "unknown",
          senderName: "Zalo Partner",
          text: rawText,
          timestamp: Date.now(),
          rawPayload: { userText, senderId },
        };
        const handled = await this.kernel.autoReply.handleIncomingMessage(normalized);
        if (handled) {
          logger.info(`[CoreKernel] Zalo incoming message auto-responded. Skipping agent loop.`);
          return;
        }

        await this.kernel.dispatch("agent_input", userText);
      },
      { channel: "zalo", userId: senderId, traceId: `zalo-${senderId || "unknown"}-${Date.now()}` }
    );
  };

  private onTelegramMessage = async (msg: NormalizedMessage) => {
    const pending = HITLGuard.getPendingByChannel("telegram");
    if (pending) {
      const cleanText = msg.text.trim().toLowerCase();
      if (["yes", "y", "ok", "oke", "okay", "okey", "duyệt", "đồng ý", "approve", "có", "co"].includes(cleanText)) {
        HITLGuard.respond(pending.id, true);
        return;
      } else if (
        ["no", "n", "hủy", "từ chối", "reject", "cancel", "huy", "không", "khong"].includes(cleanText)
      ) {
        HITLGuard.respond(pending.id, false);
        return;
      }
    }

    TraceContext.runWithContext(
      async () => {
        const blockReason = this.kernel.securityGateway.validateIncoming(msg.channel, msg.senderId);
        if (blockReason) {
          logger.warn(`[RemoteControl] 🛡️ Blocked: ${blockReason}`);
          return;
        }

        const handled = await this.kernel.autoReply.handleIncomingMessage(msg);
        if (handled) {
          logger.info(`[RemoteControl] Telegram message auto-responded. Skipping agent loop.`);
          return;
        }

        logger.info(`📱 [RemoteControl] Telegram command from ${msg.senderName}: "${msg.text}"`);
        const enrichedMessage = `[Tin nhắn từ Telegram điện thoại]: ${msg.text}`;

        const sessionId = this.kernel.sessions.getOrCreateSession(msg.senderId, msg.channel).id;
        this.kernel.sessions.appendMessage(sessionId, msg);

        const intent = await this.kernel.nlTranslator.translate(msg.text);
        if (intent.action !== "unknown" && intent.confidence > 0.8) {
          logger.info(`[RemoteControl] NL translated to IDE action: ${intent.action}`);
        }

        await this.kernel.dispatch("agent_input", enrichedMessage);
      },
      { channel: "telegram", userId: msg.senderId, traceId: `tele-${msg.senderId}-${Date.now()}` }
    );
  };

  private onMetaMessage = async (msg: NormalizedMessage) => {
    const pending = HITLGuard.getPendingByChannel("meta");
    if (pending) {
      const cleanText = msg.text.trim().toLowerCase();
      if (["yes", "y", "ok", "oke", "okay", "okey", "duyệt", "đồng ý", "approve", "có", "co"].includes(cleanText)) {
        HITLGuard.respond(pending.id, true);
        return;
      } else if (
        ["no", "n", "hủy", "từ chối", "reject", "cancel", "huy", "không", "khong"].includes(cleanText)
      ) {
        HITLGuard.respond(pending.id, false);
        return;
      }
    }

    const blockReason = this.kernel.securityGateway.validateIncoming(msg.channel, msg.senderId);
    if (blockReason) return;

    logger.info(`📱 [RemoteControl] Meta command from ${msg.senderName}: "${msg.text}"`);
    const enrichedMessage = `[Tin nhắn từ Messenger/IG]: ${msg.text}`;

    const sessionId = this.kernel.sessions.getOrCreateSession(msg.senderId, msg.channel).id;

    const intent = await this.kernel.nlTranslator.translate(msg.text);
    if (intent.action !== "unknown" && intent.confidence > 0.8) {
      logger.info(`[RemoteControl] NL translated to IDE action: ${intent.action}`);
    }

    TraceContext.runWithContext(
      async () => {
        const handled = await this.kernel.autoReply.handleIncomingMessage(msg);
        if (handled) {
          logger.info(`[RemoteControl] Meta message auto-responded. Skipping agent loop.`);
          return;
        }

        this.kernel.sessions.appendMessage(sessionId, msg);
        await this.kernel.dispatch("agent_input", enrichedMessage);
      },
      { channel: "meta", userId: msg.senderId, traceId: `meta-${msg.senderId}-${Date.now()}` }
    );
  };

  private onMetaPostback = async (postback: { senderId: string; payload: string }) => {
    logger.info(`[MetaBridge] Received postback: ${postback.payload}`);
    if (postback.payload.startsWith("approve:") || postback.payload.startsWith("reject:")) {
      const [action, id] = postback.payload.split(":");
      this.kernel.approvalEngine.resolveApproval(id, action === "approve");
    }
  };

  private onTelegramCallbackQuery = async (query: {
    queryId: string;
    senderId: string;
    data: string;
    chatId?: number;
    messageId?: number;
  }) => {
    const { data, chatId, messageId } = query;

    if (data.startsWith("approve:") || data.startsWith("reject:")) {
      const parts = data.split(":");
      const approved = parts[0] === "approve";
      const approvalId = parts[1];

      if (approvalId.startsWith("hitl-")) {
        import("../../security/HITLGuard")
          .then((m) => m.HITLGuard.respond(approvalId, approved))
          .catch((e: unknown) => {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[CoreKernel] Failed to load HITLGuard for approval response: ${errMsg}`);
          });
      } else {
        this.kernel.approvalEngine.resolveApproval(approvalId, approved);
      }

      if (chatId && messageId) {
        const statusText = approved ? "✅ **APPROVED** — Đã phê duyệt." : "❌ **REJECTED** — Đã từ chối.";
        this.kernel.telegram.editMessage(String(chatId), messageId, statusText).catch(() => {});
      }
    }
  };

  private onCdpApprovalRequired = async (payload: { text: string; selector: string }) => {
    logger.info(`[CDP] 🔔 IDE yêu cầu phê duyệt: "${payload.text}"`);

    const risk = this.kernel.securityGateway.classifyRisk(payload.text);
    const approvalId = this.kernel.approvalEngine.createApproval(
      "antigravity",
      payload.text,
      `IDE button detected: ${payload.selector}`,
      risk
    );

    try {
      await this.kernel.approvalEngine.forwardToChannel(
        approvalId,
        this.kernel.telegram,
        this.kernel.getDefaultRemoteSenderId()
      );
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.warn(`[CDP] Could not forward approval to Telegram: ${errMsg}`);
    }

    await this.kernel.dispatch("ui_broadcast", {
      name: "exec_approval_required",
      data: { approvalId, toolName: "IDE", command: payload.text, reason: payload.selector },
    });
  };

  private onApprovalGranted = async (approval: { source: string }) => {
    if (approval.source === "antigravity" && this.kernel.cdpBridge.isConnected()) {
      logger.info(`[CDP] ✅ Remote approval granted — clicking button in IDE`);
      try {
        await this.kernel.cdpBridge.clickApprovalButton(true);
      } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
        logger.error(`[CDP] Failed to click approval button: ${errMsg}`);
      }
    }
  };

  private onApprovalDenied = async (approval: { source: string }) => {
    if (approval.source === "antigravity" && this.kernel.cdpBridge.isConnected()) {
      logger.info(`[CDP] ❌ Remote approval denied — clicking reject in IDE`);
      try {
        await this.kernel.cdpBridge.clickApprovalButton(false);
      } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
        logger.error(`[CDP] Failed to click reject button: ${errMsg}`);
      }
    }
  };

  private onAudioInput = (float32: Float32Array) => {
    if (this.kernel.vadBridge && this.kernel.vadBridge.isReady) {
      this.kernel.whisperNode.pushAudioChunkOnly(float32);
      this.kernel.vadBridge.pushAudioSamples(float32);
    } else {
      logger.debug("[Audio] VAD not ready, using legacy pushAudioChunk");
      this.kernel.whisperNode.pushAudioChunk(float32);
    }
  };

  private onWakeWordTriggered = () => {
    if (this.kernel.presence === "AWAY") {
      logger.info("[CoreKernel] Wake word triggered but user is AWAY. Ignoring.");
      return;
    }
    logger.info(`[CoreKernel] Wake word triggered from frontend (ONNX WASM)`);

    this.kernel.ui.broadcastUIEvent("wake_word_detected", { trailingText: "" });

    const responseText = "Dạ, em nghe đây ạ!";
    this.kernel.voiceEngine?.speak(responseText).catch((e: Error) => {
      logger.error(`[CoreKernel] Wake word speech failed: ${e}`);
    });
    this.kernel.ui.broadcastUIEvent("ai_spoken_response", { text: responseText });
  };

  private onTranscriptionPartial = async (partialText: string) => {
    this.kernel.ui.broadcastUIEvent("transcription_partial", { text: partialText });

    const wordCount = partialText.trim().split(/\s+/).length;
    if (wordCount >= 5) {
      logger.debug(`[v23 Speculative RAG] 🔮 Pre-warming context for: "${partialText.substring(0, 50)}..."`);
      this.kernel.agentLoop.speculativeWarm(partialText).catch(() => {});
    }
  };

  private onInterrupt = () => {
    logger.warn(`[CoreKernel] 🛑 HARD INTERRUPT from UI. Kill LLM + TTS + VRAM.`);
    this.kernel.voiceEngine?.preempt?.();
    this.kernel.agentLoop.bargeIn();
    this.kernel.whisperNode.flush();
    this.kernel.ui.broadcastUIEvent("audio_ducking", { volume: 1.0 });
  };

  private onAudioPlayStarted = () => {
    logger.info("[CoreKernel] 🔇 UI playing audio -> emitting play_started to voiceEngine");
    this.kernel.voiceEngine?.emit("play_started");
  };

  private onAudioPlayFinished = () => {
    logger.info("[CoreKernel] 🔊 UI playing audio -> emitting play_finished to voiceEngine");
    this.kernel.voiceEngine?.emit("play_finished");
  };

  private resetIdleTimeout() {
    const k = this.kernel;
    const idleTimeout = k.getIdleTimeout();
    if (idleTimeout) clearTimeout(idleTimeout);
    const newTimeout = setTimeout(() => {
      if (k.voiceMode === "ACTIVE") {
        logger.info("[CoreKernel] 💤 Auto-Sleep: 30s timeout reached, returning to IDLE mode.");
        k.voiceMode = "IDLE";
        k.ui.broadcastUIEvent("voice_mode_changed", { mode: "IDLE" });
      }
    }, 30000);
    k.setIdleTimeout(newTimeout);
  }

  private onTranscriptionReady = async (text: string) => {
    const { isBackchannel } = await import("../../utils/BackchannelDetector");

    let sanitized = text.replace(/[,\s]*(Dạ|dạ|Em|em|Ạ|ạ)[,\s]*$/gi, "").trim();
    sanitized = sanitized
      .replace(/^(Dạ[,\s]+em|Dạ)[,\s]+/gi, "")
      .replace(/[,\s]+(Dạ[,\s]+em|Dạ|ạ|em|nhé|nha|ạ)[,\s]*$/gi, "")
      .trim();

    if (!sanitized) {
      this.kernel.ui.broadcastUIEvent("audio_ducking", { volume: 1.0 });
      return;
    }

    if (sanitized.length <= 1) {
      logger.debug(`[CoreKernel] 🔇 Ignored single-char noise: "${sanitized}"`);
      this.kernel.ui.broadcastUIEvent("audio_ducking", { volume: 1.0 });
      return;
    }

    this.kernel.ui.broadcastUIEvent("transcription_ready", { text: sanitized });

    if (this.kernel.voiceMode === "IDLE") {
      const wakeRegex = /(hey liva|hi liva|liva ơi|ê liva|hello liva)/i;
      if (wakeRegex.test(sanitized)) {
        logger.info(`[WakeWord] 🔔 Ánh thức thành công! Chuyển sang ACTIVE.`);
        this.kernel.voiceMode = "ACTIVE";
        this.kernel.ui.broadcastUIEvent("voice_mode_changed", { mode: "ACTIVE" });
        this.resetIdleTimeout();

        try {
          if (this.kernel.voiceEngine) {
            this.kernel.voiceEngine.preempt?.();
            await this.kernel.voiceEngine.speak("Dạ, em nghe sếp!");
          }
        } catch (e) {
          logger.error(e, "[WakeWord] Error playing wake response");
        }
        return;
      } else {
        logger.debug(`[WakeWord] Ignored background speech: "${sanitized}"`);
        return;
      }
    }

    this.resetIdleTimeout();

    if (isBackchannel(sanitized)) {
      logger.info(`[v23 Stage 2] 🔊 Backchannel detected: "${sanitized}" → Resume TTS (no abort)`);
      this.kernel.ui.broadcastUIEvent("audio_ducking", { volume: 1.0 });
      return;
    }

    TraceContext.run(async () => {
      logger.info(`[v23 Stage 2] 🛑 Real speech detected: "${sanitized.substring(0, 50)}" → Hard Abort`);
      this.kernel.voiceEngine?.preempt?.();
      this.kernel.agentLoop.bargeIn();
      this.kernel.ui.broadcastUIEvent("audio_ducking", { volume: 1.0 });

      await this.kernel.dispatch("agent_input", sanitized);
    }, `voice-${Date.now()}`);
  };

  private onSuspendPeripherals = () => {
    logger.warn(`[Z-MAS] 🛑 Singularit Mode! Đóng băng Thanh quản và Mắt để tối ưu 100% VRAM cho 26B!`);
    this.kernel.voiceEngine?.preempt?.();
    this.kernel.whisperNode.flush();
  };

  private onResumePeripherals = () => {
    logger.info(`[Z-MAS] 🟢 Expert đã xả VRAM. Kích hoạt lại Thanh quản và Lỗ tai...`);
  };

  private onVoiceEngineAudioBuffer = (buffer: Buffer) => {
    if (this.kernel.presence === "AWAY") return;
    this.kernel.ui.broadcastTTSAudio(buffer);
  };

  private onSttFallbackActivated = () => {
    logger.warn("[CoreKernel] 🔄 STT circuit open → activating Web Speech API fallback on frontend");
    this.kernel.ui.broadcastUIEvent("stt_fallback_activated", {});
  };

  private onSttFallbackDeactivated = () => {
    logger.info("[CoreKernel] ✅ STT circuit closed → deactivating Web Speech API fallback");
    this.kernel.ui.broadcastUIEvent("stt_fallback_deactivated", {});
  };

  private onWebSpeechTranscription = async (text: string) => {
    if (!text || typeof text !== "string" || text.trim().length === 0) return;
    const sanitized = text.trim();
    logger.info(`[CoreKernel] 🎤 Web Speech fallback transcription: "${sanitized.substring(0, 60)}"`);
    TraceContext.run(async () => {
      await this.kernel.dispatch("agent_input", sanitized);
    }, `web-speech-${Date.now()}`);
  };

  private onGetMemoryData = async (ws: WebSocket) => {
    try {
      if (!this.kernel.memory || !this.kernel.memory.db) {
        logger.warn("[CoreKernel] UI requested memory data but DB is not ready yet.");
        this.kernel.ui.sendMemoryData(ws, { l0: [], l0_5: "", facts: [], events: [], vectors: [] });
        return;
      }

      const l0 = await this.kernel.memory.getShortTermHistory();
      const l0_5 = await this.kernel.memory.getSessionState();
      const facts = this.kernel.memory.getAllFacts();

      interface DBEventRow {
        eventId: string;
        timestamp: number;
        phi_facts?: string;
        phi_entities?: string;
        psi_sentiment?: string;
        psi_intent?: string;
        psi_relational?: string;
        rawUserMsg?: string;
        rawAiReply?: string;
        domain?: string;
        category?: string;
        trace_keywords?: string;
        last_accessed_at?: number;
        consolidation_status?: string;
      }

      interface DBVectorRow {
        id: number;
        vec_id: string;
        type: string;
        content: string;
        domain?: string;
        category?: string;
        trace_keywords?: string;
        file_target?: string;
        created_at: number;
        last_accessed_at?: number;
        source_event_ids?: string;
      }

      const events = this.kernel.memory.db
        .prepare("SELECT * FROM events ORDER BY timestamp DESC LIMIT 100")
        .all() as unknown as DBEventRow[];
      const mappedEvents = events.map((row) => ({
        eventId: row.eventId,
        timestamp: row.timestamp,
        phi: {
          facts: JSON.parse(row.phi_facts || "[]") as unknown[],
          entities: JSON.parse(row.phi_entities || "[]") as unknown[],
        },
        psi: {
          sentiment: row.psi_sentiment || "",
          intent: row.psi_intent || "",
          relational: row.psi_relational || "",
        },
        rawUserMsg: row.rawUserMsg || "",
        rawAiReply: row.rawAiReply || "",
        domain: row.domain || "General",
        category: row.category || "Uncategorized",
        traceKeywords: JSON.parse(row.trace_keywords || "[]") as string[],
        lastAccessedAt: row.last_accessed_at || 0,
        consolidationStatus: row.consolidation_status || "consolidated",
      }));

      const vectors = this.kernel.memory.db
        .prepare("SELECT * FROM vectors_meta ORDER BY created_at DESC LIMIT 100")
        .all() as unknown as DBVectorRow[];
      const mappedVectors = vectors.map((row) => ({
        id: row.id,
        vecId: row.vec_id,
        type: row.type,
        content: row.content,
        domain: row.domain || "General",
        category: row.category || "Uncategorized",
        traceKeywords: JSON.parse(row.trace_keywords || "[]") as string[],
        fileTarget: row.file_target || "",
        createdAt: row.created_at,
        lastAccessedAt: row.last_accessed_at || 0,
        sourceEventIds: JSON.parse(row.source_event_ids || "[]") as string[],
      }));

      this.kernel.ui.sendMemoryData(ws, {
        l0,
        l0_5,
        facts,
        events: mappedEvents,
        vectors: mappedVectors,
      });
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.error(`[UI] Failed to get memory data: ${errMsg}`);
    }
  };

  private onConsolidateMemory = async (ws: WebSocket, payload: { force?: boolean }) => {
    try {
      logger.info(`[UI] 🧠 Manual memory consolidation triggered (force: ${payload?.force === true})`);
      const count = await this.kernel.memory.consolidateNow(payload?.force === true);
      ws.send(JSON.stringify({ event: "consolidation_complete", payload: { consolidated: count } }));
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.error(`[UI] Failed manual consolidation: ${errMsg}`);
    }
  };

  private onDeleteMemoryFact = (_ws: WebSocket, payload: { key: string }) => {
    try {
      const deleted = this.kernel.memory.deleteStructuredFact(payload.key);
      this.kernel.ui.broadcastUIEvent("fact_deleted", { key: payload.key, success: deleted });
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.error(`[UI] Failed to delete fact: ${errMsg}`);
    }
  };

  private onGetSkillsList = (ws: WebSocket) => {
    const whitelistData = this.kernel.registry.whitelist.getAll();
    const skills = this.kernel.registry.getAllSkills().map((s) => {
      const isOpen = this.kernel.registry.circuitBreaker.getOpenCircuits().has(s.name);
      const errorMsg = isOpen ? this.kernel.registry.circuitBreaker.getCircuitError(s.name) : null;
      const wlEntry = whitelistData[s.name];
      const isEnabled = wlEntry ? wlEntry.enabled : true;
      return {
        name: s.name,
        description: s.description,
        isCoreSkill: s.isCoreSkill || false,
        category: s.category || (s.isCoreSkill ? "Core" : "Extension"),
        status: !isEnabled ? "disabled" : isOpen ? "error" : "active",
        enabled: isEnabled,
        errorMsg: errorMsg,
      };
    });
    this.kernel.ui.sendSkillsList(ws, skills);
  };

  private onTestSkill = async (ws: WebSocket, payload: { name: string }) => {
    const skillName = payload.name;
    logger.info(`[UI] Đang chạy kiểm tra chi tiết kĩ năng: ${skillName}`);

    const testResult = await this.kernel.skillOrchestrator.performDiagnostic(skillName);

    if (ws.readyState === 1) {
      ws.send(
        JSON.stringify({
          event: "skill_check_result",
          payload: {
            name: skillName,
            ...testResult,
          },
        })
      );
    }

    const whitelistData = this.kernel.registry.whitelist.getAll();
    const skills = this.kernel.registry.getAllSkills().map((s) => {
      const isOpen = this.kernel.registry.circuitBreaker.getOpenCircuits().has(s.name);
      const wlEntry = whitelistData[s.name];
      const isEnabledVal = wlEntry ? wlEntry.enabled : true;
      return {
        name: s.name,
        description: s.description,
        isCoreSkill: s.isCoreSkill || false,
        category: s.category || (s.isCoreSkill ? "Core" : "Extension"),
        status: !isEnabledVal ? "disabled" : isOpen ? "error" : "active",
        enabled: isEnabledVal,
        errorMsg: isOpen ? this.kernel.registry.circuitBreaker.getCircuitError(s.name) : null,
      };
    });
    this.kernel.ui.sendSkillsList(ws, skills);
  };

  private onTestAllSkills = async (ws: WebSocket) => {
    logger.info("[UI] Bắt đầu chạy kiểm tra toàn bộ kĩ năng...");
    const allSkills = this.kernel.registry.getAllSkills();

    const CONCURRENCY_LIMIT = 5;
    const queue = [...allSkills];

    const runWorker = async () => {
      while (queue.length > 0) {
        const skill = queue.shift();
        if (!skill) break;

        const skillName = skill.name;
        const testResult = await this.kernel.skillOrchestrator.performDiagnostic(skillName);

        if (ws.readyState === 1) {
          ws.send(
            JSON.stringify({
              event: "skill_check_result",
              payload: {
                name: skillName,
                ...testResult,
              },
            })
          );
        }
      }
    };

    const workers = Array.from({ length: CONCURRENCY_LIMIT }, runWorker);
    await Promise.all(workers);

    if (ws.readyState === 1) {
      ws.send(
        JSON.stringify({
          event: "all_skills_check_complete",
          payload: { success: true },
        })
      );
    }

    const whitelistData = this.kernel.registry.whitelist.getAll();
    const skills = this.kernel.registry.getAllSkills().map((s) => {
      const isOpen = this.kernel.registry.circuitBreaker.getOpenCircuits().has(s.name);
      const wlEntry = whitelistData[s.name];
      const isEnabledVal = wlEntry ? wlEntry.enabled : true;
      return {
        name: s.name,
        description: s.description,
        isCoreSkill: s.isCoreSkill || false,
        category: s.category || (s.isCoreSkill ? "Core" : "Extension"),
        status: !isEnabledVal ? "disabled" : isOpen ? "error" : "active",
        enabled: isEnabledVal,
        errorMsg: isOpen ? this.kernel.registry.circuitBreaker.getCircuitError(s.name) : null,
      };
    });
    this.kernel.ui.sendSkillsList(ws, skills);
  };

  private onToggleSkill = async (ws: WebSocket, payload: { name: string; enabled: boolean }) => {
    logger.info(`[UI] Toggling skill ${payload.name}: ${payload.enabled ? "ENABLED" : "DISABLED"}`);
    this.kernel.registry.whitelist.setEnabled(payload.name, payload.enabled);
    this.kernel.ui.emit("get_skills_list", ws);
  };

  private onToggleAllSkills = async (ws: WebSocket, payload: { enabled: boolean }) => {
    logger.info(`[UI] Bulk toggle all skills: ${payload.enabled ? "ENABLED" : "DISABLED"}`);
    const allSkills = this.kernel.registry.getAllSkills();
    this.kernel.registry.whitelist.bulkSet(allSkills.map((s) => ({ name: s.name, enabled: payload.enabled })));
    this.kernel.ui.emit("get_skills_list", ws);
  };

  private onGetTasks = (ws: WebSocket) => {
    const sm = this.kernel.memory.getStructuredMemoryInstance();
    if (!sm) {
      this.kernel.ui.sendTasksList(ws, []);
      return;
    }
    const tasks = sm.getTasks();
    this.kernel.ui.sendTasksList(ws, tasks);
  };

  private onGetAiConfig = async (ws: WebSocket) => {
    const ai = await this.kernel.loadAIConfig();
    this.kernel.ui.sendAIConfig(ws, ai);
  };

  private onUpdateAiConfig = async (
    ws: WebSocket,
    payload: { ai?: Record<string, unknown> } | Record<string, unknown>
  ) => {
    try {
      const next = await this.kernel.mergeAIConfig(payload);
      await this.kernel.persistConfigPatch({ ai: next });
      this.kernel.ui.sendAIConfig(ws, next);
      this.kernel.ui.broadcastUIEvent("ai_config_updated", { ai: next });
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      this.kernel.ui.broadcastUIEvent("system_busy", { message: `AI config update failed: ${errMsg}` });
    }
  };

  private onTestAiConnection = async (
    ws: WebSocket,
    payload: { provider?: string; baseUrl?: string; apiKey?: string; model?: string }
  ) => {
    const ai = await this.kernel.loadAIConfig();
    const provider = String(payload?.provider ?? ai.provider ?? "local");
    const baseUrl = String(payload?.baseUrl ?? ai.cloudBaseUrl ?? "");
    const apiKey = String(payload?.apiKey ?? ai.cloudApiKey ?? "");
    const model = String(payload?.model ?? ai.cloudModel ?? ai.routerModel ?? "");

    let ok = false;
    let detail = "";
    try {
      if (provider === "cloud") {
        const url = baseUrl.replace(/\/$/, "") || "https://api.openai.com/v1";
        const res = await safeFetch(
          `${url}/models`,
          {
            headers: apiKey ? { Authorization: `Bearer ${apiKey}` } : undefined,
          },
          5000
        );
        ok = res.ok;
        detail = ok ? `Cloud API reachable (${model || "default"})` : `Cloud API HTTP ${res.status}`;
      } else {
        const orchestratorStatus = this.kernel.agentLoop.Orchestrator.getStatus();
        const port = orchestratorStatus.routerPort || 8000;
        const res = await safeFetch(`http://127.0.0.1:${port}/v1/models`, {}, 4000);
        ok = res.ok;
        detail = ok ? `Local model reachable (port ${port})` : `Local engine HTTP ${res.status}`;
      }
    } catch (e: unknown) {
      detail = e instanceof Error ? e.message : String(e);
    }
    this.kernel.ui.sendAIConfig(ws, { ...ai, testResult: { ok, detail } });
  };

  private onGetVoiceStatus = async (ws: WebSocket) => {
    this.kernel.ui.sendVoiceStatus(ws, await this.kernel.getVoiceStatus());
  };

  private onGetVoiceProfiles = (ws: WebSocket) => {
    this.kernel.ui.sendVoiceProfiles(ws, this.kernel.getVoiceProfiles());
  };

  private onSelectVoiceProfile = async (ws: WebSocket, payload: { profile?: string }) => {
    const voice = await this.kernel.loadVoiceConfig();
    const profileId = String(payload?.profile ?? voice.activeProfile ?? "vi-VN-HoaiMyNeural");
    const next = { ...voice, activeProfile: profileId };
    await this.kernel.persistConfigPatch({ voice: next });
    this.kernel.ui.sendVoiceStatus(ws, next);
    this.kernel.ui.broadcastUIEvent("voice_status_updated", { voice: next });
    if (this.kernel.voiceEngine) {
      (this.kernel.voiceEngine as { setVoiceProfile?: (profile: string) => void }).setVoiceProfile?.(profileId);
    }
  };

  private onStartVoiceTraining = async (
    ws: WebSocket,
    payload: { profile?: string; language?: string; sampleRate?: number }
  ) => {
    const voice = await this.kernel.loadVoiceConfig();
    const next = {
      ...voice,
      trainingEnabled: true,
      activeProfile: String(payload?.profile ?? voice.activeProfile ?? "default"),
      language: String(payload?.language ?? voice.language ?? "vi-VN"),
      sampleRate: Number(payload?.sampleRate ?? voice.sampleRate ?? 16000),
    };
    await this.kernel.persistConfigPatch({ voice: next });
    this.kernel.ui.sendVoiceStatus(ws, { ...next, trainingState: "started" });
  };

  private onStopVoiceTraining = async (ws: WebSocket) => {
    const voice = await this.kernel.loadVoiceConfig();
    const next = { ...voice, trainingEnabled: false };
    await this.kernel.persistConfigPatch({ voice: next });
    this.kernel.ui.sendVoiceStatus(ws, { ...next, trainingState: "stopped" });
  };

  private onAddTask = (ws: WebSocket, payload: { title: string; description?: string; priority?: string }) => {
    const sm = this.kernel.memory.getStructuredMemoryInstance();
    if (!sm) return;
    const id = `task_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`;
    sm.addTask({ id, title: payload.title, description: payload.description, priority: payload.priority });
    this.kernel.ui.sendTasksList(ws, sm.getTasks());

    if (payload.description?.trim()) {
      this.kernel.ui.emit("task_plan_chat", ws, { taskId: id, message: payload.description });
    }
  };

  private onTaskPlanChat = async (ws: WebSocket, payload: { taskId: string; message: string }) => {
    const { taskId, message } = payload;
    if (!taskId || !message?.trim()) return;

    const sm = this.kernel.memory.getStructuredMemoryInstance();
    if (!sm) return;
    const tasks = sm.getTasks();
    const task = tasks.find((t: { id: string }) => t.id === taskId);
    if (!task) return;

    if (!this.taskPlanHistories.has(taskId)) {
      this.taskPlanHistories.set(taskId, []);
    }
    const history = this.taskPlanHistories.get(taskId)!;
    history.push({ role: "user", content: message });

    const userProfile = (await this.kernel.memory.getUserProfile()) || {};
    const userLang = userProfile.language || "vi-VN";

    const now = new Date();
    const systemPrompt = `Bạn là trợ lý lập kế hoạch của người dùng. Nhiệm vụ: hỗ trợ lên lịch trình chi tiết.
Thời gian hiện tại: ${now.toLocaleString(userLang as string, { timeZone: "Asia/Ho_Chi_Minh" })}
Kế hoạch: "${task.title}"
${task.description ? `Initial description: ${task.description}` : ""}

QUY TẮC:
1. Nếu thiếu thông tin quan trọng (thời gian cụ thể, địa điểm, ngân sách, phương tiện, v.v.), hãy HỎI NGẮN GỌN (1-2 câu).
2. Khi đã đủ thông tin, hãy tóm tắt kế hoạch chi tiết theo dạng timeline/bullet points và kết thúc bằng dòng:
   [PLAN_COMPLETE]
   (theo sau bởi nội dung kế hoạch hoàn chỉnh)
3. TRẢ LỜI BẰNG NGÔN NGỮ: ${userLang}. Ngắn gọn, thân thiện.
4. KHÔNG bao giờ bịa thông tin — chỉ dùng thông tin người dùng cung cấp.`;

    const messages = [{ role: "system", content: systemPrompt }, ...history];

    try {
      let aiReply = "Xin lỗi, tôi không thể trả lời lúc này.";
      const USE_NATIVE_IPC = ConfigManager.getInstance().isNativeMode;

      if (USE_NATIVE_IPC) {
        const { NativeIPCClient } = await import("../../utils/NativeIPCClient");
        const client = new NativeIPCClient();
        const completion = await client.chat.completions.create({
          model: "local-ghost-router",
          messages: messages as unknown as import("../../utils/NativeIPCClient").ChatMessage[],
          temperature: 0.4,
          max_tokens: 800,
          stream: false,
        });
        aiReply = (completion as NativeIPCChatResponse).choices[0]?.message?.content?.trim() || aiReply;
      } else {
        const OpenAIClient = (await import("openai")).default;
        const port = this.kernel.agentLoop.Orchestrator.routerPort;
        const client = new OpenAIClient({
          baseURL: `http://127.0.0.1:${port}/v1`,
          apiKey: "local-ghost-router",
          timeout: 15000,
          maxRetries: 1,
        });

        const completion = await client.chat.completions.create({
          model: "local-ghost-router",
          messages: messages as unknown as OpenAI.Chat.ChatCompletionMessageParam[],
          temperature: 0.4,
          max_tokens: 800,
          stream: false,
        });
        aiReply = completion.choices[0]?.message?.content?.trim() || aiReply;
      }

      history.push({ role: "assistant", content: aiReply });

      if (aiReply.includes("[PLAN_COMPLETE]")) {
        const planContent = aiReply.split("[PLAN_COMPLETE]").pop()?.trim() || aiReply.replace("[PLAN_COMPLETE]", "").trim();
        const cleanReply = aiReply.replace("[PLAN_COMPLETE]", "").trim();

        sm.updateTask(taskId, { description: planContent, status: "pending" });
        this.taskPlanHistories.delete(taskId);

        if (ws.readyState === 1) {
          ws.send(JSON.stringify({ event: "task_plan_reply", payload: { taskId, message: cleanReply, done: true } }));
        }
        this.kernel.ui.sendTasksList(ws, sm.getTasks());
      } else {
        if (ws.readyState === 1) {
          ws.send(JSON.stringify({ event: "task_plan_reply", payload: { taskId, message: aiReply, done: false } }));
        }
      }
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.warn(`[TaskPlanner] LLM call failed: ${errMsg}`);
      if (ws.readyState === 1) {
        ws.send(
          JSON.stringify({
            event: "task_plan_reply",
            payload: { taskId, message: "⚠️ Không thể kết nối AI. Vui lòng thử lại.", done: false },
          })
        );
      }
    }
  };

  private onUpdateTask = (ws: WebSocket, payload: { id: string; updates?: Record<string, unknown> }) => {
    const sm = this.kernel.memory.getStructuredMemoryInstance();
    if (!sm) return;
    sm.updateTask(payload.id, payload.updates || {});
    this.kernel.ui.sendTasksList(ws, sm.getTasks());
  };

  private onDeleteTask = (ws: WebSocket, payload: { id: string }) => {
    const sm = this.kernel.memory.getStructuredMemoryInstance();
    if (!sm) return;
    sm.deleteTask(payload.id);
    this.kernel.ui.sendTasksList(ws, sm.getTasks());
  };

  private onExecuteTask = (ws: WebSocket, payload: { id: string; title: string }) => {
    const sm = this.kernel.memory.getStructuredMemoryInstance();
    if (!sm) return;
    sm.updateTask(payload.id, { status: "in-progress" });
    this.kernel.ui.emit("user_input", payload.title);
    this.kernel.ui.sendTasksList(ws, sm.getTasks());
  };

  private onForceGc = (ws: WebSocket) => {
    logger.info("[CoreKernel] 🧹 Ép chạy dọn dẹp bộ nhớ (Garbage Collection)...");
    if (global.gc) {
      global.gc();
    }
    this.kernel.addTelemetryLog("info", "🧹 Đã ép chạy dọn rác hệ thống (V8 Garbage Collection) thành công.");
    this.kernel.ui.emit("get_system_status", ws);
  };

  private onTriggerGitnexusIndex = (_ws: WebSocket) => {
    logger.info("[CoreKernel] ⚡ Kích hoạt quét mã nguồn GitNexus thủ công...");
    this.kernel.gitNexusIndexer.triggerIndex();
    this.kernel.addTelemetryLog("info", "⚡ Đã kích hoạt quét chỉ mục mã nguồn GitNexus chạy ngầm.");
  };

  private onReloadSkills = async (ws: WebSocket) => {
    logger.info("[CoreKernel] 🔄 Tải lại toàn bộ kỹ năng hệ thống...");
    await this.kernel.registry.registerLocalSkills();
    this.kernel.addTelemetryLog("info", "🔄 Đã tải lại toàn bộ kỹ năng hệ thống thành công.");
    this.kernel.ui.emit("get_skills_list", ws);
  };

  private onGetSystemStatus = async (ws: unknown) => {
    let networkStatus = "Disconnected";

    try {
      const active: string[] = [];
      const nets = os.networkInterfaces();
      for (const [name, interfaces] of Object.entries(nets)) {
        if (!interfaces) continue;
        for (const net of interfaces) {
          if (!net.internal && net.family === "IPv4") active.push(`${name}`);
        }
      }
      if (active.length > 0) networkStatus = "Online (" + active.join(", ") + ")";
    } catch {
      /* ignore */
    }

    try {
      if (!this.kernel.cachedStaticStats) {
        this.kernel.cachedStaticStats = { cpuModel: "Đang quét...", totalRamGB: 0, diskInfo: "Đang quét..." };
        const cpus = os.cpus();
        if (cpus && cpus.length > 0) this.kernel.cachedStaticStats.cpuModel = cpus[0].model.trim();
        this.kernel.cachedStaticStats.totalRamGB = Math.round(os.totalmem() / 1024 / 1024 / 1024);

        if (os.platform() === "win32") {
          import("child_process")
            .then((cp) => {
              cp.exec("wmic diskdrive get model,size /format:csv", { timeout: 2000 }, (err, stdout) => {
                if (!err && stdout) {
                  const lines = stdout
                    .toString()
                    .split("\n")
                    .map((l) => l.trim())
                    .filter((l) => l.length > 0 && !l.toLowerCase().includes("model,size") && l.includes(","));
                  const disks = lines
                    .map((l) => {
                      const parts = l.split(",");
                      if (parts.length >= 3) {
                        const model = parts[1].trim();
                        const sizeStr = parts[2].trim();
                        const sizeGB = Math.round(parseInt(sizeStr) / 1024 / 1024 / 1024);
                        return `${model} (${sizeGB}GB)`;
                      }
                      return "";
                    })
                    .filter((d) => d.length > 0);

                  if (disks.length > 0)
                    this.kernel.cachedStaticStats!.diskInfo = `${disks.length} Ổ cứng: ` + disks.join(", ");
                }
              });
            })
            .catch(() => {});
        } else if (os.platform() === "darwin") {
          import("child_process")
            .then((cp) => {
              cp.exec("df -lh / | tail -1 | awk '{print $2, $4}'", { timeout: 2000 }, (err, stdout) => {
                if (!err && stdout) {
                  const parts = stdout.trim().split(/\s+/);
                  if (parts.length >= 2) {
                    this.kernel.cachedStaticStats!.diskInfo = `Ổ đĩa hệ thống (Tổng: ${parts[0]}B, Trống: ${parts[1]}B)`;
                  }
                }
              });
            })
            .catch(() => {});
        }
      }
    } catch {
      /* ignore */
    }

    const isNativeMode = ConfigManager.getInstance().isNativeMode;
    const orchestratorStatus = this.kernel.agentLoop.Orchestrator.getStatus();
    const processMemory = process.memoryUsage();

    const tcpPing = async (
      port: number,
      host = "127.0.0.1",
      timeoutMs = 1500
    ): Promise<{ ok: boolean; latencyMs: number }> => {
      const net = await import("net");
      const start = Date.now();
      return new Promise((resolve) => {
        const sock = net.createConnection({ port, host, timeout: timeoutMs }, () => {
          sock.destroy();
          resolve({ ok: true, latencyMs: Date.now() - start });
        });
        sock.on("error", () => resolve({ ok: false, latencyMs: Date.now() - start }));
        sock.on("timeout", () => {
          sock.destroy();
          resolve({ ok: false, latencyMs: Date.now() - start });
        });
      });
    };

    let aiEngineHealth: { status: string; latencyMs: number; detail: string; modelLoaded?: string } = {
      status: "offline",
      latencyMs: -1,
      detail: "",
    };
    try {
      const aiStart = Date.now();
      if (isNativeMode) {
        const aiRes = await safeFetch("http://127.0.0.1:8100/health", {}, 2000).catch(() => null);
        if (aiRes && aiRes.ok) {
          aiEngineHealth = { status: "online", latencyMs: Date.now() - aiStart, detail: "Native gRPC (HTTP health OK)" };
        } else {
          const tcp = await tcpPing(8100);
          aiEngineHealth = {
            status: tcp.ok ? "online" : "offline",
            latencyMs: tcp.latencyMs,
            detail: tcp.ok ? "Native gRPC (TCP OK)" : "gRPC port 8100 unreachable",
          };
        }
      } else {
        const port = orchestratorStatus.routerPort || 8000;
        const res = await safeFetch(`http://127.0.0.1:${port}/v1/models`, {}, 2000);
        const body = (await res.json()) as Record<string, unknown>;
        const models = Array.isArray(body.data) ? body.data : [];
        const modelId = (models[0] as Record<string, unknown>)?.id || "unknown";
        aiEngineHealth = {
          status: "online",
          latencyMs: Date.now() - aiStart,
          detail: `llama-server (port ${port})`,
          modelLoaded: String(modelId),
        };
      }
    } catch {
      aiEngineHealth.detail = isNativeMode ? "gRPC port 8100 unreachable" : "llama-server not responding";
    }

    let voiceHealth: { status: string; latencyMs: number; detail: string } = { status: "offline", latencyMs: -1, detail: "" };
    try {
      const tcp = await tcpPing(8002);
      voiceHealth = {
        status: tcp.ok ? "online" : "offline",
        latencyMs: tcp.latencyMs,
        detail: tcp.ok ? "Edge-TTS Python" : "Port 8002 unreachable",
      };
    } catch {
      voiceHealth.detail = "Port 8002 check failed";
    }

    const gatewayHealth = {
      status: "online" as const,
      latencyMs: 0,
      detail: "WebSocket Server",
      wsClients: this.kernel.ui.connectedClientCount,
      skillsLoaded: this.kernel.registry.getAllSkills().length,
    };

    const orchestratorReady = this.kernel.agentLoop.Orchestrator.isReady();
    const orchestratorHealth = {
      status: orchestratorReady ? "online" : "offline",
      detail: orchestratorReady
        ? `Ready (port ${orchestratorStatus.routerPort})`
        : "NOT READY — AgentLoop blocked!",
    };

    let memoryHealth: { status: string; detail: string } = { status: "offline", detail: "" };
    try {
      const sm = this.kernel.memory.getStructuredMemoryInstance();
      const factCount = sm.count;
      memoryHealth = { status: "online", detail: `SQLite OK (${factCount} facts)` };
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      memoryHealth = { status: "offline", detail: `SQLite: ${errMsg.substring(0, 60)}` };
    }

    const whisperHealth = {
      status: this.kernel.whisperNode ? "online" : "offline",
      detail: this.kernel.whisperNode ? "NemotronSTT active" : "Not initialized",
    };

    const voiceConfig = await this.kernel.loadVoiceConfig();

    const remoteControlEnabled = this.kernel.securityGateway.isRemoteControlEnabled();
    const telegramConfigured = !!process.env.TELEGRAM_BOT_TOKEN;
    const zaloConfigured =
      !!process.env.ZALO_OA_ACCESS_TOKEN && !process.env.ZALO_OA_ACCESS_TOKEN.includes("NHẬP_TOKEN");
    const remoteHealth = {
      enabled: remoteControlEnabled,
      telegram: {
        configured: telegramConfigured,
        status:
          remoteControlEnabled && telegramConfigured
            ? "online"
            : telegramConfigured
              ? "standby"
              : "not_configured",
      },
      zalo: {
        configured: zaloConfigured,
        status: zaloConfigured ? "online" : "not_configured",
      },
    };

    const status = {
      model: ConfigManager.getInstance().env.EXPERT_MODEL_NAME,
      provider: ConfigManager.getInstance().aiProvider,
      engineMode: isNativeMode ? "native_grpc" : "llama_http",
      uptime: process.uptime(),
      memoryUsage: processMemory.heapUsed,
      rssMemory: processMemory.rss,
      externalMemory: processMemory.external,
      telemetry: this.kernel.telemetryLogs,
      osStats: {
        cpuModel: this.kernel.cachedStaticStats?.cpuModel || "Đang quét...",
        totalRamGB: this.kernel.cachedStaticStats?.totalRamGB || 0,
        networkStatus,
        diskInfo: this.kernel.cachedStaticStats?.diskInfo || "Đang quét...",
      },
      healthChecks: {
        aiEngine: aiEngineHealth,
        voiceEngine: voiceHealth,
        gateway: gatewayHealth,
        orchestrator: orchestratorHealth,
        memory: memoryHealth,
        whisper: whisperHealth,
        remoteControl: remoteHealth,
      },
      voice: voiceConfig,
    };
    this.kernel.ui.sendSystemStatus(ws as import("ws").WebSocket, status);
  };

  private onResetMemory = async (ws: WebSocket) => {
    logger.warn("[CoreKernel] 🧹 Nhận lệnh RESET MEMORY từ Dashboard!");
    const result = await this.kernel.memory.resetAllMemory();
    if (ws.readyState === 1) {
      ws.send(
        JSON.stringify({
          event: "memory_reset_result",
          payload: result,
        })
      );
    }
    if (result.success) {
      this.kernel.ui.broadcastUIEvent("memory_reset_complete", {});
    }
  };

  private onCameraFrame = (payload: { image: string; timestamp: number }) => {
    this.kernel.latestCameraFrame = payload.image;
    logger.info(`[Camera] 📸 Nhận frame webcam (${Math.round(payload.image.length / 1024)}KB)`);
  };
}
