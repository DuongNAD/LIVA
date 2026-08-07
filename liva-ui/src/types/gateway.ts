/**
 * gateway.ts — hình dạng gói tin đi qua WebSocket Gateway (cổng 8002)
 * ====================================================================
 * Tách ra khỏi `WidgetApp.vue` ngày 07/08/2026, khi `useWidgetTransport.ts`
 * được bóc ra thành composable (mục A31-04).
 *
 * Vì sao phải tách chứ không để nguyên trong `.vue`: composable vận chuyển
 * cần chính những kiểu này cho tham số `onJsonMessage`, mà kiểu khai bên trong
 * một Single-File Component thì không import được từ ngoài. Bản tách đầu tiên
 * lách bằng `any` — làm đỏ cổng `eslint --max-warnings 0` (luật
 * `no-explicit-any`), và quan trọng hơn là ném mất kiểu ở đúng ranh giới
 * không-tin-được: dữ liệu vào đây đến từ mạng.
 */

/** Model avatar mà widget truyền xuống engine 3D/2D. */
export interface WidgetModelConfig {
  filename: string;
  type?: string;
  format?: string;
}

export interface WidgetAvatarConfig {
  engineMode?: string;
  activeModel?: WidgetModelConfig;
  vrmModel?: string;
  live2dModel?: string;
}

/** Bản nháp tin nhắn đang chờ người dùng xác nhận. */
export interface MessageDraft {
  draft_id: string;
  platform: string;
  display_name: string;
  handle: string;
  text: string;
}

/**
 * Payload lỏng từ Gateway — mỗi event chỉ dùng một phần các trường dưới đây.
 * `text`/`audio` khai là bắt buộc vì code truy cập trực tiếp trong đúng nhánh
 * event của chúng (chỉ là kiểu, không đổi hành vi lúc chạy).
 */
export type GatewayPayload = {
  ui?: { avatarMode?: string; activeModel?: WidgetModelConfig };
  avatarMode?: string;
  activeModel?: WidgetModelConfig;
  avatar?: WidgetAvatarConfig;
  text: string;
  textChunk?: string;
  isThought?: boolean;
  audio: string;
  volume?: number;
  enabled?: boolean;
  level?: string;
  fps?: number;
  /** `message:pending_response` — bản nháp đang chờ xác nhận, mới nhất trước. */
  drafts?: MessageDraft[];
  /** `message:confirm_response` — câu mô tả việc đã gửi. */
  detail?: string;
  /** `<lệnh>_error` — lý do thất bại, do lõi soạn. */
  error?: string;
  tool?: string;
  label?: string;
  ok?: boolean;
  data?: unknown;
  reason?: string;
};

/** Một gói tin WebSocket từ Gateway (một số event đặt field ngay ở gốc). */
export type GatewayMessage = GatewayPayload & {
  event?: string;
  payload: GatewayPayload;
};
