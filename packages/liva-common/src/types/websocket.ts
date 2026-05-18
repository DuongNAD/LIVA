/**
 * liva-common/src/types/websocket.ts — WebSocket Event Contract (SSOT)
 * =====================================================================
 * Defines all valid WebSocket event names and their payload shapes.
 * Used by both UIController (Gateway) and useGateway (UI) to ensure
 * compile-time safety across the communication boundary.
 */

// ─── Client → Gateway (Requests) ───
export type WSClientEvent =
    // Config
    | 'get_config'
    | 'update_config'
    | 'get_ai_config'
    | 'update_ai_config'
    | 'test_ai_connection'
    // Voice
    | 'get_voice_status'
    | 'get_voice_profiles'
    | 'select_voice_profile'
    | 'start_voice_training'
    | 'stop_voice_training'
    // Avatar
    | 'get_avatar_models'
    | 'import_avatar_folder'
    | 'delete_avatar_model'
    // Skills
    | 'get_skills_list'
    | 'toggle_skill'
    | 'toggle_all_skills'
    // System
    | 'get_system_status'
    // User Profile
    | 'get_user_profile'
    | 'update_user_profile'
    // Tasks
    | 'get_tasks'
    | 'add_task'
    | 'update_task'
    | 'delete_task'
    | 'execute_task'
    | 'task_plan_chat'
    // User interaction
    | 'user_voice_command'
    | 'camera_frame'
    | 'wake_word_triggered'
    // Env/Integrations
    | 'get_env_config'
    | 'save_env_config'
    // Memory
    | 'reset_memory'
    // File Explorer
    | 'explorer_ls'
    | 'explorer_cat'
    // Utility
    | 'ping';

// ─── Gateway → Client (Responses / Broadcasts) ───
export type WSServerEvent =
    // Config
    | 'config_data'
    | 'config_updated'
    | 'config_error'
    | 'ai_config'
    | 'ai_config_updated'
    // Voice
    | 'voice_status'
    | 'voice_profiles'
    // Avatar
    | 'avatar_models_list'
    // Skills
    | 'skills_list'
    // System
    | 'system_status'
    | 'gpu_setup_progress'
    // User Profile
    | 'user_profile'
    | 'profile_updated_success'
    | 'profile_update_error'
    // Tasks
    | 'tasks_list'
    | 'task_plan_reply'
    // Env/Integrations
    | 'env_config_data'
    // Memory
    | 'memory_reset_result'
    // Chat Stream
    | 'ai_response_start'
    | 'ai_response_chunk'
    | 'ai_response_end'
    // Thinking/Tool UI
    | 'thinking_start'
    | 'thinking_end'
    | 'tool_executing'
    | 'tool_result'
    // File Explorer
    | 'explorer_ls_result'
    | 'explorer_cat_result'
    | 'explorer_error'
    // Utility
    | 'pong';

// ─── Unified Message Envelope ───
export interface WSMessage<P = unknown> {
    event: WSClientEvent | WSServerEvent;
    payload?: P;
}

// ─── Typed Payload Helpers (extend as needed) ───
export interface TaskPlanReplyPayload {
    taskId: string;
    message: string;
    done: boolean;
}

export interface GPUSetupPayload {
    status: string;
}

export interface AvatarModelsPayload {
    models3d: Array<Record<string, unknown>>;
    models2d: Array<Record<string, unknown>>;
}

export interface EnvConfigPayload {
    content: string;
    vault?: Record<string, string>;
}
