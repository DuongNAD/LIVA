export type TtsEngine = "kokoro" | "browser" | "none";

export interface KernelConfig {
    readonly enableVoice: boolean;
    readonly enableTelegram: boolean;
    readonly enableGitNexus: boolean;
    readonly ttsEngine: TtsEngine;
}

export type KernelConfigOverrides = Partial<KernelConfig>;

export const DEFAULT_KERNEL_CONFIG: KernelConfig = Object.freeze({
    enableVoice: false,
    enableTelegram: false,
    enableGitNexus: false,
    ttsEngine: "none",
});

export function createDefaultKernelConfig(overrides: KernelConfigOverrides = {}): KernelConfig {
    return Object.freeze({
        ...DEFAULT_KERNEL_CONFIG,
        ...overrides,
    });
}
