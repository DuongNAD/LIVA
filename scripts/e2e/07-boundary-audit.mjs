/**
 * Test Group 7: Architectural Boundary Audit & Feature Mapping (F1 - F15)
 *
 * Explains and validates why Features F1 - F15 CANNOT and MUST NOT be tested over
 * an unprivileged external WebSocket connection (CommandPrincipal::WebSocketRemote),
 * and maps each feature to its genuine in-process Rust test coverage (`cargo test`).
 */

import assert from 'node:assert/strict'
import { connectWebSocket } from './helpers.mjs'

// Bản đồ dưới đây là TÀI LIỆU, không phải phép kiểm — nó được in ra chứ không
// được đếm vào tổng số test. Lý do phải nói rõ: bản đầu tiên của mục 7 khai một
// mảng 15 phần tử rồi `assert.equal(map.length, 15)`, và khai một mảng 9 lệnh
// rồi `assert.equal(REMOTE_COMMANDS.length, 9)` — tức assert lại đúng thứ vừa
// tự gõ ra. Hai phép đó không thể đỏ dù lõi Rust có đổi thế nào, đúng bằng
// chứng bệnh mà cả bộ test này sinh ra để chữa. Một bảng tra cứu có ích vẫn là
// tài liệu; đội lốt test đang pass thì nó thành lời khai man.
export const FEATURE_BOUNDARY_MAP = [
  {
    feature: 'F1: SQLite WAL Pool Concurrency',
    allowlist: 'In-Process Only (AppState.db)',
    reason: 'Kết nối pool nội bộ, không mở cổng trực tiếp cho remote client.',
    rustTestTarget: 'liva-native-core/src/db.rs',
  },
  {
    feature: 'F2: RAG Hybrid Search & RRF',
    allowlist: 'In-Process / Dashboard Tickets',
    reason: 'Thuật toán tìm kiếm lai chạy trong tiến trình, không phơi bày API thô.',
    rustTestTarget: 'liva-native-core/src/agent/graph.rs',
  },
  {
    feature: 'F3: Swarm DAG Orchestrator',
    allowlist: 'In-Process Actors',
    reason: 'Luồng điều phối tác tử chạy bằng async Tokio actors.',
    rustTestTarget: 'liva-native-core/src/agent/',
  },
  {
    feature: 'F4: Secret Scrubber & Stronghold Vault',
    allowlist: 'In-Process (DPAPI / Keystore)',
    reason: 'Mã hoá khoá thiết bị và lọc bí mật thực thi ở tầng boot và ghi đĩa.',
    rustTestTarget: 'liva-native-core/src/crypto.rs',
  },
  {
    feature: 'F5: Vision & Screen Capture',
    allowlist: 'LocalCli / TauriWidget',
    reason: 'Chụp màn hình bị khoá đối với WebSocketRemote để chống rò rỉ dữ liệu qua mạng.',
    rustTestTarget: 'liva-native-core/src/vision/',
  },
  {
    feature: 'F6: MCP Tool Integration',
    allowlist: 'LocalCli / WebSocketDashboard',
    reason: 'Thực thi công cụ hệ thống đòi hỏi quyền Dashboard/LocalCli.',
    rustTestTarget: 'liva-native-core/src/mcp/',
  },
  {
    feature: 'F7: PKM Daily Memory Consolidation',
    allowlist: 'In-Process Background Worker',
    reason: 'Hợp nhất bộ nhớ chạy ngầm trong tiến trình lõi.',
    rustTestTarget: 'liva-native-core/src/memory_consolidation.rs',
  },
  {
    feature: 'F8: Morning Intelligence Briefing',
    allowlist: 'In-Process Background Worker',
    reason: 'Bộ thu thập tin tức chạy định kỳ trên background task.',
    rustTestTarget: 'liva-native-core/src/integrations/',
  },
  {
    feature: 'F9: Skill Store Governance',
    allowlist: 'TauriDashboard / WebSocketDashboard',
    reason: 'Quản trị kỹ năng yêu cầu quyền bảng điều khiển cục bộ.',
    rustTestTarget: 'liva-native-core/src/skills/',
  },
  {
    feature: 'F10: Autonomous Coding Swarm',
    allowlist: 'LocalCli / Subagent IPC',
    reason: 'Điều phối code chạy qua luồng IPC tiêu chuẩn.',
    rustTestTarget: 'liva-native-core/src/commands/',
  },
  {
    feature: 'F11: Multi-Platform Messaging Bridge',
    allowlist: 'Telegram / LocalCli',
    reason: 'Kết nối Telegram chạy qua webhook/long-polling daemon riêng.',
    rustTestTarget: 'liva-native-core/src/telegram.rs',
  },
  {
    feature: 'F12: BI Text-to-SQL Analytics',
    allowlist: 'LocalCli / TauriDashboard',
    reason: 'Truy vấn phân tích dữ liệu yêu cầu quyền quản trị.',
    rustTestTarget: 'liva-native-core/src/cognitive/',
  },
  {
    feature: 'F13: CRM & ERP Synchronization',
    allowlist: 'LocalCli / Background Sync',
    reason: 'Đồng bộ hóa doanh nghiệp chạy ngầm.',
    rustTestTarget: 'liva-native-core/src/integrations/',
  },
  {
    feature: 'F14: Zero-Trust Security & PDG Auditing',
    allowlist: 'Static Analysis & Build Layer',
    reason: 'Kiểm toán luồng mã nguồn chạy ở tầng biên dịch / CI.',
    rustTestTarget: 'liva-native-core/src/authorization.rs',
  },
  {
    feature: 'F15: Smart DevOps Pipeline Doctor',
    allowlist: 'LocalCli / In-Process Diagnostics',
    reason: 'Chẩn đoán CI/CD thực hiện qua công cụ dòng lệnh nội bộ.',
    rustTestTarget: 'liva-native-core/src/preflight.rs',
  },
]

// Nguồn sự thật: `liva-native-core/src/authorization.rs:122-131`. Giữ hai danh
// sách này khớp với file đó — chúng là ĐẦU VÀO của phép dò qua socket bên dưới,
// không phải thứ được assert vào chính nó.
const REMOTE_COMMANDS = [
  'ping',
  'status',
  'llm:health_check',
  'chat:completion',
  'voice:stt_start',
  'voice:stt_chunk',
  'voice:stt_stop',
  'voice:tts_speak',
  'voice:tts_stop',
]

// Mẫu lệnh nằm NGOÀI `REMOTE_COMMANDS` (thuộc WIDGET_COMMANDS/DASHBOARD_COMMANDS).
// Chọn khác với mục 2 để không trùng phủ: mục 2 dò mcp/vision/preflight.
const NON_REMOTE_SAMPLE = [
  'get_config',
  'get_tasks',
  'get_skills_list',
  'get_user_profile',
  'get_avatar_models',
  'get_system_status',
]

const KHONG_CO_QUYEN = 'not authorized'

export async function runBoundaryAuditReport(reporter, port) {
  reporter.startSection(7, 'Architectural Boundary Audit & Coverage Mapping')

  // In bản đồ ranh giới ra như tài liệu — không đếm vào tổng số test.
  console.log('  📋 Ranh giới F1-F15 (tài liệu, không phải phép kiểm):')
  for (const item of FEATURE_BOUNDARY_MAP) {
    console.log(`     · ${item.feature} → ${item.allowlist} · phủ thật ở ${item.rustTestTarget}`)
  }
  console.log('')

  const conn = await connectWebSocket({ port })
  if (!conn.ok) {
    reporter.test('Khởi tạo kết nối WebSocket cho mục 7', () => {
      throw new Error(`Không kết nối được gateway: ${conn.reason}`)
    })
    reporter.endSection()
    return
  }
  const ws = conn.ws

  try {
    // 7.1 — đỏ nếu ai đó BỚT một lệnh khỏi REMOTE_COMMANDS.
    // Không assert "phải thành công": thiếu payload hay thiếu model vẫn là lỗi
    // hợp lệ và vẫn chứng minh lệnh đã qua được cổng phân quyền. Thứ duy nhất
    // bị cấm ở đây là bị chặn vì KHÔNG CÓ QUYỀN.
    const biChan = []
    for (const cmd of REMOTE_COMMANDS) {
      const res = await ws.sendEvent(cmd, {}, 5000)
      const loi = String(res?.payload?.error ?? '')
      if (loi.includes(KHONG_CO_QUYEN)) biChan.push(cmd)
    }
    reporter.test(
      `7.1 Cả ${REMOTE_COMMANDS.length} lệnh REMOTE_COMMANDS đều qua được cổng phân quyền qua socket thật`,
      () => {
        assert.deepEqual(
          biChan,
          [],
          `Lệnh lẽ ra phải cho phép nhưng bị chặn vì thiếu quyền: ${biChan.join(', ')}`,
        )
      },
    )

    // 7.2 — đỏ nếu ai đó THÊM một lệnh vào REMOTE_COMMANDS, tức nới ranh giới
    // bảo mật ra ngoài mà không ai nhận ra.
    const lotLuoi = []
    for (const cmd of NON_REMOTE_SAMPLE) {
      const res = await ws.sendEvent(cmd, {}, 5000)
      const loi = String(res?.payload?.error ?? '')
      if (!loi.includes(KHONG_CO_QUYEN)) lotLuoi.push(`${cmd} → ${res?.event}`)
    }
    reporter.test(
      `7.2 Cả ${NON_REMOTE_SAMPLE.length} lệnh ngoài REMOTE_COMMANDS đều bị WebSocketRemote từ chối`,
      () => {
        assert.deepEqual(
          lotLuoi,
          [],
          `Lệnh lẽ ra phải bị chặn nhưng lọt qua: ${lotLuoi.join(' | ')}`,
        )
      },
    )
  } finally {
    ws.close()
  }

  reporter.endSection()
}
