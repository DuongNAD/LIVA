/**
 * VramCostEstimator
 * =================
 * Maps task categories to their expected memory footprint in MB.
 * Automatically resolves footprint depending on model configurations.
 */

export enum TaskType {
    LLM_ROUTER = "LLM_ROUTER",
    LLM_EXPERT = "LLM_EXPERT",
    LLM_STREAM = "LLM_STREAM",
    AUDIO_VOICE = "AUDIO_VOICE",
    LIVE2D_RENDER_BUFFER = "LIVE2D_RENDER_BUFFER",
    TELEMETRY = "TELEMETRY",
}

const DEFAULT_VRAM_COSTS: Record<TaskType, number> = {
    [TaskType.LLM_ROUTER]: 5300,
    [TaskType.LLM_EXPERT]: 6700,
    [TaskType.LLM_STREAM]: 4500,
    [TaskType.AUDIO_VOICE]: 300,
    [TaskType.LIVE2D_RENDER_BUFFER]: 150,
    [TaskType.TELEMETRY]: 50,
};

export class VramCostEstimator {
    /**
     * Retrieve estimated VRAM cost for a task type in MB.
     */
    public static get(taskType: TaskType): number {
        return DEFAULT_VRAM_COSTS[taskType] ?? 0;
    }
}
