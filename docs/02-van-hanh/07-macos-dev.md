---
title: "07 — Phát triển LIVA trên macOS"
updated: 2026-08-25
commit: 3a8d5001
status: living
owns:
  - macos-dev
covers:
  - scripts/start_all.sh
  - scripts/e2e-gateway-ci.mjs
  - liva-native-core/src/governor.rs
  - liva-native-core/tests/artifact_trust.rs
  - liva-ui/package.json
stale-ok: a0153135
---

# 07 — Phát triển LIVA trên macOS

> Áp dụng cho nhánh `mac-v2` (tách từ main f35961cf). Bản Windows dùng
> `scripts/start_all.ps1`; tài liệu này mô tả đường tương đương trên macOS.

## Khởi động nhanh

```bash
npm install                 # cài workspace (liva-ui, liva-desktop, packages/*)
npm run setup:models        # tải model GGUF/ONNX (chạy một lần)
cargo build --release       # trong liva-native-core — lần đầu ~6 phút
npm run dev:mac             # = bash scripts/start_all.sh
```

Kiểm tra môi trường mà không đụng tiến trình nào:

```bash
npm run dev:mac:check       # = bash scripts/start_all.sh --check-only
```

## Script `scripts/start_all.sh`

Port của `scripts/start_all.ps1`, giữ nguyên trình tự:

1. **Port Guard** — giải phóng cổng 5173 (UI/Vite) và 8002 (native core) bằng
   `lsof`, chỉ kill tiến trình thuộc checkout này (đối chiếu đường dẫn qua
   `lsof -p`), chặn port bị tiến trình lạ giữ.
2. **Preflight** — chạy binary core đã build với `--preflight` (môi trường chạy:
   GPU, espeak-ng, ffmpeg, vec0, khóa mã hóa) và `npm run doctor` (file model).
3. **Khởi động** — UI dev server chạy nền (`/tmp/liva-ui-dev.log`), chờ port
   5173 sẵn sàng rồi mới `npx tauri dev --no-dev-server`.
4. **Cleanup** khi thoát (trap EXIT/INT/TERM) — tắt UI server và các
   `llama-server` sinh ra trong phiên (giữ nguyên instance có trước).

## GPU: Metal tự động, không cần feature flag

Trên Apple Silicon, llama.cpp được biên dịch với **Metal bật mặc định**
(`GGML_USE_METAL` xuất hiện sẵn trong build log của `llama-cpp-sys-2`) — không
cần tương đương `--features cuda` như bản Windows. Nhánh `cuda`/`vulkan` trong
`liva-native-core/Cargo.toml` chỉ dùng cho Windows/Linux.

## Số đo CPU thật cho Governor trên macOS

`governor.rs` trước đây trả `None` cho mọi nền không phải Windows (nhánh
`GetSystemTimes`). Trên macOS đã hiện thực `cpu_sample()`:

- Tải hệ thống: `host_statistics64(HOST_CPU_LOAD_INFO)` — tick scheduler Mach.
- Phần CPU của chính LIVA: `getrusage(RUSAGE_SELF)` — micro-giây cộng dồn,
  quy về % trên đồng hồ wall giữa hai lần lấy mẫu (hai đơn vị KHÔNG trộn).
- Không thêm crate: FFI trực tiếp với libSystem, cùng tinh thần nhánh Windows.

Test: `cargo test --lib governor::` → `macos_sample_nam_trong_khoang_hop_ly`.

## vec0 trên macOS: hash theo nền trong trust manifest

`load_sqlite_vec` xác minh SHA-256 của `vec0.dylib` trước khi nạp (fail-closed).
Trước đây manifest ghim **một** hash duy nhất (của DLL Windows) nên macOS luôn
rơi vào "no such module: vec0". Nay `data/models-manifest.json` có bảng:

```json
"vec0": {
  "sha256": "<hash windows dll>",
  "platforms": {
    "windows-x64": "<…>",
    "darwin-arm64": "<…>"
  }
}
```

Loader chọn theo `runtime_artifact_platform_key()` (`artifact_trust.rs`). Khi
nâng phiên bản npm `sqlite-vec`, phải băm lại binary của từng nền và cập nhật
bảng này — `npm run test:installer` sẽ bắt hash sai định dạng.

Lưu ý riêng của build darwin-arm64: với vector đối cực (ví dụ toàn +1.0 vs toàn
-1.0), vec0 trả `distance = NULL`. `search_similar_vectors` xử bằng cách gán
khoảng cách +∞ (score = 0) thay vì panic — kết quả vẫn được trả về đúng thứ tự.

## Khác biệt đã biết so với bản Windows

| Hạng mục | Windows | macOS |
|---|---|---|
| Script khởi động | `start_all.ps1` | `scripts/start_all.sh` |
| npm script | `dev` | `dev:mac`, `dev:mac:check` |
| GPU backend | CUDA (`--features cuda`) | Metal tự động |
| Wake-word training | `train-wakeword.ps1` | chưa port |
| Installer | Tauri bundle `.msi` | chưa cấu hình bundle `.dmg` |
| Binary lõi | `liva-native-core.exe` | `liva-native-core` (không hậu tố) |
| Junction/symlink trong test | junction = thư mục thật | symlink — xoá bằng `remove_file` |

## Ba bẫy đã trả giá khi đưa cổng kiểm sang macOS (25/08/2026)

Cả ba đều **xanh trên CI** và sẽ tiếp tục xanh, vì workflow chạy `windows-latest`. Nói cách khác đây không phải hồi quy — đây là **vùng mù của phép đo**.

**1. `scripts/e2e-gateway-ci.mjs` hardcode `.exe`.** Đường dẫn binary ghi thẳng `target/<profile>/liva-native-core.exe`, nên trên macOS script dừng ngay ở `Không thấy binary`. Đã sửa thành tên theo `process.platform`; sau khi vá chạy **8/8 đạt**. Hệ quả cần nhớ: **gate e2e ở bước 20/25 của CI là gate chỉ-Windows**, đừng đọc nó như bằng chứng đa nền tảng.

**2. `liva-native-core/tests/artifact_trust.rs` xoá symlink bằng `fs::remove_dir`.** Trên Windows `link` là *junction* — một thư mục thật — nên `remove_dir` đúng. Trên unix nó là symlink tới thư mục, và `remove_dir` trả `ENOTDIR` ⇒ panic ở **teardown**.

> ⚠️ **Đừng đọc nhầm cái này thành lỗ hổng bảo mật trên macOS.** Assertion thật của test — canonicalization phải **từ chối** symlink thoát khỏi trust root — **đã pass**. Chỉ đoạn dọn dẹp sau đó sai. Sau khi tách `#[cfg(windows)]` / `#[cfg(unix)]`: **5/5 ok**.

**3. `@vitest/coverage-istanbul` chỉ là *optional peer*.** Không workspace nào khai báo nó, nên gate coverage tái lập được trên máy Windows đã có sẵn cây phụ thuộc cũ, nhưng **không** tái lập được sau một `npm ci` sạch trên nền khác. Đã khai báo `4.1.5` trong `liva-ui/package.json` (khớp đúng version vitest). Không hạ ngưỡng nào; đo lại được **80,86 % line**.

**Còn đỏ trên máy macOS này, cố ý không vá:** `preflight::n_gpu_layers_bang_0_khong_bao_gio_la_xanh` — thư mục `models/` chưa có file `.gguf`/`.onnx` nào. Thông điệp test đã ghi sẵn *"tải model xong dòng này tự xanh"*; cách xử đúng là `npm run setup:models`, không phải hạ ngưỡng.

📌 Bảng đo đầy đủ và ngữ cảnh hồi quy: [Đường cơ sở §1 — đo lại 25/08](../03-danh-gia/05-nang-cap-toan-dien.md)
