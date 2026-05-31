/**
 * @file src/core/CoreKernelAuthority.ts
 * Decoupled state manager and authority token granter.
 * Serves as the shared context to prevent Circular Dependencies.
 */
import { AgentPhase, AuthorityToken } from "../types/AgentTypes";

export class KernelAuthorityToken<S extends AgentPhase> implements AuthorityToken<S> {
    public readonly phase: S;
    #secret: string;

    constructor(phase: S, secret: string) {
        this.phase = phase;
        this.#secret = secret;
    }

    public isValid(expectedPhase: S, expectedSecret: string): boolean {
        return this.phase === expectedPhase && this.#secret === expectedSecret;
    }
}

export class CoreKernelAuthority {
    #kernelSecret = "LIVA_KERNEL_CORE_99X_ALPHA";
    static #instance: CoreKernelAuthority;

    private constructor() { }

    public static getInstance(): CoreKernelAuthority {
        if (!CoreKernelAuthority.#instance) {
            CoreKernelAuthority.#instance = new CoreKernelAuthority();
        }
        return CoreKernelAuthority.#instance;
    }

    public issueToken<S extends AgentPhase>(phase: S): AuthorityToken<S> {
        return new KernelAuthorityToken<S>(phase, this.#kernelSecret);
    }

    public verify<S extends AgentPhase>(token: AuthorityToken<S>, phase: S): boolean {
        return token.isValid(phase, this.#kernelSecret);
    }
}
