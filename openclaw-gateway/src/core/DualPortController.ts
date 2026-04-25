import { ModelOrchestrator } from "./ModelOrchestrator";
import { logger } from "../utils/logger";

export class DualPortController {
    #orchestrator: ModelOrchestrator;
    #isExpertAwake = false;

    constructor(orchestrator: ModelOrchestrator) {
        this.#orchestrator = orchestrator;
    }

    get isExpertAwake() { return this.#isExpertAwake; }

    async ensureExpertReady(): Promise<boolean> {
        try {
            if (this.#isExpertAwake) return true;
            await this.#orchestrator.stopRouter();

            // Token issuance bound strictly to Core limits
            await this.#orchestrator.startExpert(ModelOrchestrator.getAuthorizedTokenFactory().issueToken("EXPERT_START_AUTH" as any));
            this.#isExpertAwake = true;
            return true;
        } catch (e: any) {
            logger.error("[CircuitBreaker] VRAM Overload. Expert Load Failed. Falling back to Router.", e.message);
            await this.#orchestrator.startRouter(ModelOrchestrator.getAuthorizedTokenFactory().issueToken("ROUTER_START_AUTH" as any));
            this.#isExpertAwake = false;
            return false;
        }
    }

    async releaseResources() {
        if (this.#isExpertAwake) {
            logger.info("🛡️ [CircuitBreaker] RAII Triggered: Giải phóng VRAM Expert để tránh kẹt Deadlock...");
            try {
                await this.#orchestrator.stopExpert();
            } catch (e) { void e; }
            this.#isExpertAwake = false;
        }
    }
}
