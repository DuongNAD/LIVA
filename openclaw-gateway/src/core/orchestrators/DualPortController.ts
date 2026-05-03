import { ModelOrchestrator } from "../ModelOrchestrator";
import { logger } from "../../utils/logger";
import { CoreKernelAuthority } from "../CoreKernelAuthority";

export class DualPortController {
    #orchestrator: ModelOrchestrator;
    #isExpertAwake = false;
    private authority: CoreKernelAuthority;
    private logger: any;

    constructor(orchestrator: ModelOrchestrator, authority: CoreKernelAuthority) {
        this.#orchestrator = orchestrator;
        this.authority = authority;
        this.logger = logger.child({ component: 'DualPortController' });
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
            this.logger.error("[CircuitBreaker] VRAM Overload. Expert Load Failed. Falling back to Router.", e.message);
            await this.#orchestrator.startRouter(ModelOrchestrator.getAuthorizedTokenFactory().issueToken("ROUTER_START_AUTH" as any));
            this.#isExpertAwake = false;
            return false;
        }
    }

    async releaseResources() {
        if (this.#isExpertAwake) {
            this.logger.info("🛡️ [CircuitBreaker] RAII Triggered: Giải phóng VRAM Expert để tránh kẹt Deadlock...");
            try {
                await this.#orchestrator.stopExpert();
            } catch (e) { void e; }
            this.#isExpertAwake = false;
        }
    }
}
