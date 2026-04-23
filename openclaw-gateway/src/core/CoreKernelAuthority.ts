import { AgentPhase, AuthorityToken } from "../types/AgentTypes";

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
        return new AuthorityToken<S>(phase, this.#kernelSecret);
    }

    public verify<S extends AgentPhase>(token: AuthorityToken<S>, phase: S): boolean {
        return token.isValid(phase, this.#kernelSecret);
    }
}
