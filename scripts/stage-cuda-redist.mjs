// Chép ba DLL runtime của NVIDIA cạnh binary trước khi `tauri build` đóng gói.
//
// # Vì sao cần bước này
//
// Bản `--features cuda` **load-time-link** `cudart` và `cublas`. Chỉ
// `nvcuda.dll`/`nvml.dll` đi kèm driver; ba file dưới đây thuộc CUDA Toolkit và
// phải được phát hành kèm. Thiếu chúng thì tiến trình **không khởi động nổi** —
// exit code 127, không một dòng thông báo, vì nó chết trong DLL loader trước
// khi bất kỳ mã LIVA nào chạy. Đó là chế độ hỏng tệ nhất có thể giao cho beta
// tester: bấm vào icon, không có gì xảy ra, không có gì để đọc.
//
// # Vì sao không commit thẳng vào repo
//
// 752 MB. `cublasLt64_12.dll` một mình đã 643 MB.
//
// # Vì sao đích là thư mục cạnh .exe chứ không phải `resources/`
//
// Windows DLL loader tìm trong thư mục chứa .exe, **không** tìm trong thư mục
// con. Ánh xạ `resources` của Tauri với đích là tên file trần sẽ đặt file ngay
// cạnh .exe — đúng cách `vec0.dll` đang được phát hành, nên đường này đã được
// chứng minh chạy trên chính sản phẩm này.
//
// # Dùng
//
//   node scripts/stage-cuda-redist.mjs          # dàn dựng, báo lỗi nếu thiếu
//   node scripts/stage-cuda-redist.mjs --check   # chỉ báo cáo, luôn exit 0
//   node scripts/stage-cuda-redist.mjs --clean   # xoá thư mục dàn dựng
//
// Bản CPU **không cần** chạy lệnh này: thư mục dàn dựng rỗng thì mẫu glob
// trong `tauri.conf.json` không khớp gì và bundle vẫn dựng được như cũ.
import fs from 'node:fs'
import path from 'node:path'

const ROOT = path.resolve(import.meta.dirname, '..')
const DICH = path.join(ROOT, 'liva-desktop', 'src-tauri', 'cuda-redist')
const DLL = ['cudart64_12.dll', 'cublas64_12.dll', 'cublasLt64_12.dll']

const argv = process.argv.slice(2)
const CHECK = argv.includes('--check')
const CLEAN = argv.includes('--clean')

if (CLEAN) {
  fs.rmSync(DICH, { recursive: true, force: true })
  console.log(`stage-cuda-redist: đã xoá ${path.relative(ROOT, DICH)}`)
  process.exit(0)
}

/**
 * Thư mục `bin` của CUDA Toolkit.
 *
 * `CUDA_PATH` do bộ cài đặt ra và là nguồn đúng nhất. Chỉ khi thiếu nó mới dò
 * thư mục mặc định, và khi đó lấy **bản mới nhất** — `v12.8` phải thắng `v12.1`
 * khi cả hai cùng có mặt, nên phải so theo số chứ không theo thứ tự chuỗi
 * (`v12.10` xếp trước `v12.8` nếu so chuỗi).
 */
function timBin() {
  if (process.env.CUDA_PATH) {
    const p = path.join(process.env.CUDA_PATH, 'bin')
    if (fs.existsSync(p)) return { duongDan: p, nguon: 'CUDA_PATH' }
  }
  const goc = 'C:\\Program Files\\NVIDIA GPU Computing Toolkit\\CUDA'
  if (!fs.existsSync(goc)) return null
  const ban = fs.readdirSync(goc)
    .map((ten) => ({ ten, so: (ten.match(/^v(\d+)\.(\d+)$/) || []).slice(1).map(Number) }))
    .filter((x) => x.so.length === 2)
    .sort((a, b) => b.so[0] - a.so[0] || b.so[1] - a.so[1])
  for (const b of ban) {
    const p = path.join(goc, b.ten, 'bin')
    if (fs.existsSync(p)) return { duongDan: p, nguon: `dò thư mục mặc định (${b.ten})` }
  }
  return null
}

const bin = timBin()
if (!bin) {
  const loi = 'stage-cuda-redist: KHÔNG tìm thấy CUDA Toolkit.'
  if (CHECK) { console.log(`${loi} Bản CPU không cần nó.`); process.exit(0) }
  console.error(`${loi}\n  Đặt CUDA_PATH, hoặc cài toolkit 12.x.\n  Chỉ bản --features cuda mới cần; bản CPU bỏ qua bước này.`)
  process.exit(1)
}

const thieu = DLL.filter((d) => !fs.existsSync(path.join(bin.duongDan, d)))
if (thieu.length) {
  const loi = `stage-cuda-redist: thiếu ${thieu.join(', ')} trong ${bin.duongDan}`
  if (CHECK) { console.log(loi); process.exit(0) }
  console.error(`${loi}\n  Toolkit có nhưng không đủ file — cài thiếu component cuBLAS?`)
  process.exit(1)
}

if (CHECK) {
  console.log(`stage-cuda-redist: đủ 3 DLL trong ${bin.duongDan} (${bin.nguon})`)
  process.exit(0)
}

fs.mkdirSync(DICH, { recursive: true })
let tong = 0
for (const d of DLL) {
  const tu = path.join(bin.duongDan, d)
  const den = path.join(DICH, d)
  const cd = fs.statSync(tu).size
  // Chép lại chỉ khi khác kích thước — `cublasLt64_12.dll` là 643 MB, chép thừa
  // mỗi lần build là mấy chục giây không đổi lấy gì.
  if (!fs.existsSync(den) || fs.statSync(den).size !== cd) fs.copyFileSync(tu, den)
  tong += cd
  console.log(`  ${d.padEnd(22)} ${(cd / 1048576).toFixed(1).padStart(7)} MB`)
}
console.log(`stage-cuda-redist: ${DLL.length} DLL → ${path.relative(ROOT, DICH)} (${(tong / 1048576).toFixed(1)} MB, nguồn: ${bin.nguon})`)
