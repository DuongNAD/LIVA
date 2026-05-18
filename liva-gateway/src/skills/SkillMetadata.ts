export type SkillCategory = 
  | 'core'      // Lệnh hệ thống, I/O cơ bản, dịch thuật, hệ điều hành
  | 'web'       // Duyệt web, tóm tắt, tải dữ liệu
  | 'devops'    // Chạy code, test mạng, đo đạc hệ thống
  | 'data'      // Xử lý ảnh, mã QR, định dạng dữ liệu
  | 'docs'      // PDF, báo cáo văn bản
  | 'personal'  // Sao lưu, chi tiêu, cá nhân hóa
  | 'social'    // Tin nhắn, email
  | 'agentic';  // Lập kế hoạch, suy luận sâu

export interface SkillMetadata {
    name: string;
    category?: SkillCategory;      // BẮT BUỘC dùng enum này theo chuẩn v19
    short_desc?: string;           // Tối đa 80 ký tự (Dùng cho RAG)
    description: string;
    semantic_tags?: string[];      // Từ khóa vector cho sqlite-vec
    search_keywords?: string[];    // Tương thích ngược keyword search
    requires_hitl?: boolean;       // Cờ bảo mật - Bắt buộc người dùng UI duyệt
    is_cpu_heavy?: boolean;        // Cờ hiệu năng - Cảnh báo khóa Event Loop
    isCoreSkill?: boolean;
    kit?: string;                  // Fallback for legacy dynamic gating
    parameters: any;
}
