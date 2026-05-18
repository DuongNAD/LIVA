import { VoiceOrchestrator } from "../orchestrators/VoiceOrchestrator";
import { MemoryManager } from "../../MemoryManager";
import { SkillRegistry } from "../../SkillRegistry";
import { AgentLoop } from "../AgentLoop";
import { logger } from "../../utils/logger";

export class DependencyContainer {
    private static instance: DependencyContainer;
    
    public memory: MemoryManager;
    public registry: SkillRegistry;
    public agentLoop: AgentLoop;
    public voiceOrchestrator: VoiceOrchestrator;

    private constructor() {
        this.memory = new MemoryManager("liv_async_core");
        this.registry = new SkillRegistry();
        this.agentLoop = new AgentLoop(this.memory, this.registry);
        this.voiceOrchestrator = new VoiceOrchestrator();
    }

    public static getInstance(): DependencyContainer {
        if (!DependencyContainer.instance) {
            DependencyContainer.instance = new DependencyContainer();
        }
        return DependencyContainer.instance;
    }

    public async dispose() {
        // Voice Orchestrator
        await this.voiceOrchestrator.dispose();
        
        // Memory & Agents are currently disposed by CoreKernel.shutdown() explicitly
        // based on the Strict Shutdown laws, we leave them to be commanded by CoreKernel directly.
        logger.info("[DependencyContainer] Dependencies managed safely.");
    }
}
