/**
 * liva-common/src/types/websocket.ts — WebSocket Event Contract (SSOT)
 * =====================================================================
 * Defines all valid WebSocket event names and their payload shapes.
 * Used by the Rust command plane clients and useGateway (UI) to improve
 * compile-time safety across the communication boundary.
 */

// ─── Client → Gateway (Requests) ───
//
// Nguồn sự thật: các danh sách cho phép trong `liva-native-core/src/authorization.rs`
// (SETUP_/WIDGET_/DASHBOARD_/REMOTE_COMMANDS) cộng `user_voice_command` — bí danh tầng
// WebSocket được quy về `chat:completion` ở `websocket.rs:190-192`. Đối chiếu lại 16/08/2026:
// bản trước khai 27 lệnh thật, 15 lệnh KHÔNG có handler, và **thiếu 42 lệnh có thật** —
// một hợp đồng mô tả đúng chưa tới 40% bề mặt lệnh. Sửa lệnh ở Rust thì sửa cả ở đây.
export type WSClientEvent =
    // Utility & vòng đời
    | 'ping'
    | 'echo'
    | 'status'
    | 'get_preflight_status'
    // Setup (chỉ principal TauriSetup)
    | 'setup:status'
    | 'setup:paths'
    | 'setup:fetch'
    // Config
    | 'get_config'
    | 'update_config'
    | 'get_ai_config'
    // Voice — trạng thái & hồ sơ
    | 'get_voice_status'
    | 'get_voice_profiles'
    | 'select_voice_profile'
    // Voice — đường ống STT/TTS
    | 'voice:stt_start'
    | 'voice:stt_chunk'
    | 'voice:stt_stop'
    | 'voice:tts_speak'
    | 'voice:tts_stop'
    | 'voice:set_language'
    | 'voice:list_vieneu_voices'
    | 'voice:set_vieneu_voice'
    // Avatar
    | 'get_avatar_models'
    | 'import_avatar_folder'
    | 'delete_avatar_model'
    // Skills
    | 'get_skills_list'
    | 'toggle_skill'
    | 'toggle_all_skills'
    | 'skills:list'
    | 'skills:search'
    | 'skills:signals'
    | 'skills:history'
    // System & người dùng
    | 'get_system_status'
    | 'get_user_profile'
    | 'update_user_profile'
    // Tasks
    | 'get_tasks'
    | 'add_task'
    | 'update_task'
    | 'delete_task'
    | 'task_plan_chat'
    // Chat & LLM
    | 'chat:completion'
    | 'user_voice_command'
    | 'llm:embed'
    | 'llm:health_check'
    // Memory
    | 'get_memory_data'
    | 'memory:set_fact'
    | 'memory:get_fact'
    | 'delete_memory_fact'
    | 'memory:delete_conversation'
    | 'memory:delete_subject'
    | 'memory:sweep_retention'
    | 'consolidate_memory'
    | 'reset_memory'
    | 'memory:search_hybrid'
    | 'memory:upsert_vector'
    // Vision
    | 'vision:capture'
    | 'vision:ask'
    | 'vision:add_region'
    | 'vision:remove_region'
    | 'vision:get_changed_regions'
    | 'vision:set_config'
    // Consent
    | 'consent:get'
    | 'consent:grant'
    | 'consent:revoke'
    // Danh bạ & nhắn tin
    | 'contacts:list'
    | 'contacts:upsert'
    | 'contacts:delete'
    | 'message:draft'
    | 'message:confirm'
    | 'message:cancel'
    | 'message:pending'
    | 'messenger:status'
    // Tích hợp & MCP
    | 'integrations:list'
    | 'mcp:list_tools'
    | 'mcp_client:list_servers'
    | 'mcp_client:list_tools'
    // ─────────────────────────────────────────────────────────────────────────
    // ⚠️ CHƯA CÓ HANDLER Ở BACKEND — giữ lại vì UI VẪN ĐANG GỬI, không phải vì hợp lệ.
    //
    // Không lệnh nào dưới đây có mặt trong `authorization.rs`, nên chúng rơi xuống
    // handler mặc định và trả về `<lệnh>_error` ("Unknown command"). Với người dùng
    // thì đó là một nút bấm không làm gì cả.
    //
    // XOÁ khỏi đây chỉ an toàn SAU KHI bỏ chỗ gọi ở UI hoặc hiện thực handler —
    // xoá trước sẽ làm `vue-tsc` đỏ ngay tại các dòng trên.
    //
    // Đã dọn trong cùng lần rà 16/08/2026:
    //   • 4 mục không ai gọi — `test_ai_connection`, `execute_task`, `explorer_ls`,
    //     `explorer_cat` — xoá thẳng.
    //   • `camera_frame` — xoá cả bên gửi. Lõi không có đường nhận ảnh từ client;
    //     vision của LIVA là lõi tự chụp màn hình (`vision:capture`). Vòng lặp cũ
    //     encode một frame mỗi 10 giây rồi đẩy vào hư không.
    //   • `wake_word_triggered` — **đi nhầm chiều**. Đây là event server→client:
    //     lõi phát nó ở `websocket.rs:749`, UI nhận ở `useVoicePipeline.ts:458`, và
    //     `useVoicePipeline.ts:220` ghi rõ "UI chỉ chuyển ACTIVE khi core trả
    //     `wake_word_triggered`". Đã chuyển xuống `WSServerEvent` và bỏ lời gọi
    //     `sendMsg` ở `WidgetApp.vue`.
    //   • Voice training (`start_voice_training`, `stop_voice_training`) bị chặn ở thượng nguồn
    //     (backlog item U17b, thiếu 2 model). Không tự ý hiện thực nếu chưa giải quyết U17b.
    | 'start_voice_training'
    | 'stop_voice_training';

// ─── Gateway → Client (Responses / Broadcasts) ───
export type WSServerEvent =
    // Config
    | 'config_data'
    | 'config_updated'
    | 'config_error'
    | 'ai_config'
    | 'update_ai_config'
    | 'ai_config_updated'
    // Voice
    | 'voice_status'
    | 'voice_profiles'
    // Wake word — lõi phát lên (`websocket.rs:749`) kèm `{ score, transcript }`.
    // UI nhận ở `useVoicePipeline.ts:458` và CHỈ chuyển sang ACTIVE khi nhận được
    // event này; đừng gửi nó theo chiều ngược lại.
    | 'wake_word_triggered'
    | 'wake_probe_rejected'
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
