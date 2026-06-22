import type { CoreKernel } from "../CoreKernel";
import { DependencyContainer } from "../bootstrap/DependencyContainer";
import { UIController } from "../UIController";
import { PowerMonitorService } from "../../services/PowerMonitorService";
import { HeartbeatManager } from "../HeartbeatManager";
import { ZaloPolling } from "../ZaloPolling";
import { AppWatcherService } from "../../services/AppWatcherService";
import { PresenceDetector } from "../../services/PresenceDetector";
import { AppConfig } from "../../config/AppConfig";
import { SessionOrchestrator } from "../SessionOrchestrator";
import { TelegramBridge } from "../../channels/TelegramBridge";
import { MetaBridge } from "../../channels/MetaBridge";
import { CDPBridge } from "../../bridges/CDPBridge";
import { AutoAcceptDaemon } from "../../security/AutoAcceptDaemon";
import { ApprovalEngine } from "../ApprovalEngine";
import { ChannelRouter } from "../../channels/ChannelNormalizer";
import { SecurityGateway } from "../../security/SecurityGateway";
import { VSCodeBridge } from "../../bridges/VSCodeBridge";
import { NLCommandTranslator } from "../NLCommandTranslator";
import { EmailClientManager } from "../../services/EmailClientManager";
import { AutoReplyManager } from "../../services/AutoReplyManager";
import { GitNexusIndexer } from "../../evolution/GitNexusIndexer";
import { Scheduler } from "../../kernel/Scheduler";
import { PluginSkillOrchestrator } from "./PluginSkillOrchestrator";
import { logger } from "../../utils/logger";

export class KernelDI {
  public static wire(kernel: CoreKernel): void {
    const container = DependencyContainer.getInstance();
    kernel.memory = container.memory;
    kernel.registry = container.registry;
    kernel.agentLoop = container.agentLoop;
    kernel.voiceOrchestrator = container.voiceOrchestrator;
    kernel.scheduler = Scheduler.getInstance();

    kernel.ui = new UIController();
    kernel.powerMonitor = new PowerMonitorService(kernel.ui);
    kernel.heartbeat = new HeartbeatManager(kernel.agentLoop);
    kernel.zalo = new ZaloPolling();
    kernel.appWatcher = new AppWatcherService(kernel.memory);

    kernel.presenceDetector = new PresenceDetector();

    // [v5.0] Remote Control Hub — Initialize
    const appConfig = AppConfig.get();

    kernel.sessions = new SessionOrchestrator();
    kernel.telegram = new TelegramBridge();
    kernel.meta = new MetaBridge(appConfig.META_WEBHOOK_PORT);
    kernel.cdpBridge = new CDPBridge(process.env.CDP_HOST || "127.0.0.1", appConfig.CDP_PORT);
    kernel.autoAcceptDaemon = new AutoAcceptDaemon(kernel.cdpBridge, kernel.telegram);
    kernel.telegram.setBridges(
      kernel.cdpBridge,
      kernel.autoAcceptDaemon,
      kernel.agentLoop,
      kernel.sessions,
      kernel.memory
    );
    kernel.approvalEngine = new ApprovalEngine();
    kernel.channelRouter = new ChannelRouter();
    kernel.channelRouter.register(kernel.telegram);
    kernel.channelRouter.register(kernel.meta);
    kernel.channelRouter.register(kernel.zalo);
    kernel.agentLoop.channelRouter = kernel.channelRouter;
    kernel.securityGateway = new SecurityGateway();

    // [v5.0] Phase 2 Initialize
    kernel.vscodeBridge = new VSCodeBridge(process.env.VSCODE_WS_HOST || "127.0.0.1", appConfig.VSCODE_WS_PORT);
    kernel.nlTranslator = new NLCommandTranslator();
    kernel.emailManager = new EmailClientManager();
    kernel.autoReply = new AutoReplyManager(kernel.channelRouter, kernel.sessions);

    kernel.gitNexusIndexer = new GitNexusIndexer();

    // Voice Orchestrator Bootstrap
    kernel.voiceOrchestrator.initialize(kernel.agentLoop).catch((e: Error) => logger.error(e, "Lỗi khởi tạo VoiceOrchestrator:"));

    // Set transition schema and orchestration tensor
    kernel.setTransitionSchema(new Map());
    kernel.setOrchestrationTensor({
      dimensions: [3, 3],
      getWeight: (latencyMs: number) => Math.max(0.1, 1 / (latencyMs + 1)),
      updateWeights: (_feedbackLoop: number[]) => {
        /* Tensor update logic */
      },
    });

    kernel.skillOrchestrator = new PluginSkillOrchestrator(kernel.registry, kernel.ui);
  }
}
