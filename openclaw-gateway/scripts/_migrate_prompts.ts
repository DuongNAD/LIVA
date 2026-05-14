// Script to convert final Vietnamese skill descriptions to English
// Run: npx tsx src/_migrate_prompts.ts
import * as fs from "node:fs";
import * as path from "node:path";

const TRANSLATIONS: Record<string, string> = {
  "Hành động:\n          1. 'audit': Quét toàn bộ skills...":
    "Action:\n          1. 'audit': Scan all skills...",
  "Thao tác trực tiếp (Computer Use) trên màn hình OS Windows. LIVA hoạt động như một người thật: Di chuyển chuột, click, gõ bàn phím và chụp ảnh màn hình.":
    "Direct Computer Use on Windows OS. LIVA acts as a human: Move mouse, click, type on keyboard, and take screenshots.",
  "Quản lý bộ nhớ đệm (Clipboard) của hệ điều hành. Đọc nội dung người dùng vừa copy, hoặc ghi nội dung vào bộ nhớ đệm để người dùng có thể dán (paste) ở nơi khác.":
    "OS Clipboard management. Read copied content or write content to clipboard for the user to paste elsewhere.",
  "Quản lý lịch trình (Google Calendar / Outlook). Đọc lịch rảnh và đặt lịch hẹn mới. Mọi thao tác đặt lịch (create) đều phải qua HITL Guard phê duyệt và gọi API qua safeFetch.":
    "Calendar management (Google Calendar / Outlook). Read availability and book new appointments. All creation actions require HITL Guard approval and use safeFetch API.",
  "Trích xuất văn bản từ PDF (thuần JS) và tự động Chunking vào StructuredMemory (chạy qua Background Task Lane) để tránh chặn WS Heartbeat.":
    "Extract text from PDF (pure JS) and auto-chunk into StructuredMemory (via Background Task Lane) to avoid blocking WS Heartbeat.",
  "Quản lý tiến trình hệ thống Windows. Liệt kê Top N tiến trình theo CPU/RAM, tìm kiếm tiến trình theo tên, và kết thúc (kill) tiến trình an toàn qua HITL Guard.":
    "Windows process manager. List top N processes by CPU/RAM, search by name, and safely kill processes via HITL Guard.",
  "Tương tác với hệ thống Git cục bộ. Hỗ trợ xem trạng thái (status, log, diff) và thao tác an toàn (commit, push, pull, checkout) - yêu cầu phê duyệt HITL đối với các lệnh thay đổi trạng thái.":
    "Interact with local Git. Supports status, log, diff, and safe operations (commit, push, pull, checkout) - requires HITL approval for state-changing commands.",
  "Truy vấn kiến trúc hệ thống bằng vector siêu tốc (Zero VRAM Leak)":
    "Query system architecture using ultra-fast vector search (Zero VRAM Leak)",
  "Chạy mã độc lập hoặc phân tích dữ liệu rủi ro bên trong Docker Container (Zero-Trust Sandbox) với cờ cách ly mạng và giới hạn bộ nhớ.":
    "Run standalone code or analyze risky data inside a Docker Container (Zero-Trust Sandbox) with network isolation and memory limits.",
  "Ninja Đóng gói: Nén (Zip) hoặc giải nén (Unzip) toàn bộ một dự án / thư mục một cách nhanh chóng. Hỗ trợ backup hoặc chuẩn bị file nộp bài.":
    "Zip/Unzip: Compress or extract entire project/directory quickly. Supports backup or preparing files for submission.",
  "Phân tích hình ảnh (Vision). Hệ thống sẽ tự động Hot Swap (đổi não) từ mô hình Text sang mô hình Vision (VD: LLaVA) trên VRAM để đọc ảnh.":
    "Image analysis (Vision). System auto hot-swaps from Text model to Vision model (e.g., LLaVA) on VRAM to read images.",
  "Phân tích file dữ liệu lớn (CSV/TXT) bằng Stream (Zero-Blocking) để trả về thống kê cấu trúc (dòng, cột, null counts, head 5 dòng) mà không làm đầy RAM/VRAM.":
    "Analyze large data files (CSV/TXT) via Stream (Zero-Blocking) to return structural stats (rows, columns, null counts, top 5 rows) without filling RAM/VRAM.",
  "Chuyển đổi dữ liệu giữa JSON và YAML. Hỗ trợ đọc từ file hoặc text trực tiếp. Zero-dependency (YAML parser tự viết, không cần thư viện ngoài).":
    "Convert data between JSON and YAML. Supports reading from file or direct text. Zero-dependency (custom YAML parser, no external libraries).",
  "Tính toán Hash/Checksum của file (MD5, SHA1, SHA256, SHA512) bằng Stream không tốn RAM. Hỗ trợ xác minh tính toàn vẹn file bằng cách so sánh hash kỳ vọng.":
    "Calculate file Hash/Checksum (MD5, SHA1, SHA256, SHA512) via Stream (zero RAM overhead). Supports file integrity verification by comparing expected hash.",
  "Dọn dẹp và phân loại file tự động trong một thư mục (VD: Downloads, Desktop). File sẽ được gom vào các folder: Images, Documents, Media, Archives, Setups.":
    "Auto-organize files in a directory (e.g., Downloads, Desktop). Files sorted into: Images, Documents, Media, Archives, Setups.",
  "Tương tác trực tiếp với cơ sở dữ liệu SQLite cục bộ. Chạy các lệnh SQL để trích xuất báo cáo, phân tích dữ liệu mà không cần phần mềm DB Client ngoài.":
    "Interact directly with local SQLite database. Run SQL commands for reporting and data analysis without external DB Client software.",
  "Sinh JSON cấu hình biểu đồ (ECharts) và bắn sự kiện IPC hiển thị trực tiếp lên màn hình Desktop/UI thay vì dùng code tĩnh C++ hay Python.":
    "Generate ECharts JSON config and push IPC event to display chart directly on Desktop/UI instead of static C++ or Python code.",
};

const skillsDir = path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Z]:)/, '$1')), "skills");

function processFile(filePath: string) {
  let content = fs.readFileSync(filePath, "utf-8");
  let changed = false;

  for (const [vi, en] of Object.entries(TRANSLATIONS)) {
    if (content.includes(vi)) {
      content = content.replaceAll(vi, en);
      changed = true;
    }
  }

  if (changed) {
    fs.writeFileSync(filePath, content, "utf-8");
    console.log(`✅ ${path.relative(skillsDir, filePath)}`);
  }
}

function walkDir(dir: string) {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      walkDir(fullPath);
    } else if (entry.name.endsWith(".ts") && entry.name !== "index.ts") {
      processFile(fullPath);
    }
  }
}

walkDir(skillsDir);
console.log("\n🎉 Migration complete!");
