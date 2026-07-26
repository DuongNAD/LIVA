// Chỉ mục ký hiệu (symbol) cho neo tài liệu dạng `file.rs#ten_ky_hieu`.
//
// Vì sao cần: tài liệu LIVA neo vào **số dòng**. Mỗi lần mã dịch chuyển, toạ độ
// âm thầm trỏ sai — và bộ dò cơ học vẫn xanh miễn số dòng còn nằm trong file.
// Neo theo TÊN KÝ HIỆU sống sót qua refactor: tách một hàm ra module khác thì
// tên nó vẫn thế, còn xoá/đổi tên thì bộ dò báo LỖI THẬT thay vì im lặng.
//
// Không dùng parser thật (syn/ts-morph) có chủ ý: thêm một cây phụ thuộc nặng
// cho một bộ dò chạy trong CI là cái giá sai. Regex theo dòng đủ cho mã đã
// rustfmt/eslint, và mọi trường hợp nó không nhận ra đều **hiện ra** dưới dạng
// "không tìm thấy ký hiệu" chứ không phải một kết quả sai âm thầm.

/** Ký hiệu Rust ở đầu dòng: `pub async fn x`, `struct X`, `const X`… */
const RUST_ITEM =
  /^(\s*)(?:#\[[^\]]*\]\s*)?(?:pub(?:\s*\([^)]*\))?\s+)?(?:default\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+"[^"]*"\s+)?(fn|struct|enum|trait|union|type|const|static|mod)\s+([A-Za-z_][A-Za-z0-9_]*)/
/** `impl Foo`, `impl<T> Foo<T>`, `impl Trait for Foo` → lấy tên KIỂU. */
const RUST_IMPL =
  /^(\s*)impl(?:\s*<[^>]*>)?\s+(?:[A-Za-z_][A-Za-z0-9_:<>,\s]*?\s+for\s+)?([A-Za-z_][A-Za-z0-9_]*)/
const RUST_MACRO = /^(\s*)macro_rules!\s+([A-Za-z_][A-Za-z0-9_]*)/

/** TS/JS/Vue: khai báo có tên. */
const TS_ITEM =
  /^(\s*)(?:export\s+)?(?:default\s+)?(?:declare\s+)?(?:abstract\s+)?(?:async\s+)?(?:function|class|interface|enum|type)\s+([A-Za-z_$][\w$]*)/
const TS_BINDING =
  /^(\s*)(?:export\s+)?(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*(?:[:=]|<)/

const DUOI_RUST = new Set(['rs'])
const DUOI_TS = new Set(['ts', 'tsx', 'js', 'mjs', 'cjs', 'vue'])

/** Đuôi file có hỗ trợ neo ký hiệu hay không (toml/json/yml thì không). */
export const hoTroKyHieu = (rel) => {
  const d = rel.split('.').pop()?.toLowerCase()
  return DUOI_RUST.has(d) || DUOI_TS.has(d)
}

/**
 * Bóc ký hiệu từ mã nguồn.
 *
 * Trả `[{ id, ten, kind, line }]`. Phương thức trong `impl Foo` có
 * `id = "Foo::bar"` và `ten = "bar"` — tài liệu neo được bằng cả hai, và bộ dò
 * báo lỗi khi tên trần trùng nhau trong cùng file.
 */
export function bocKyHieu(rel, noiDung) {
  const d = rel.split('.').pop()?.toLowerCase()
  const dong = noiDung.replace(/\r\n/g, '\n').split('\n')
  const ra = []

  if (DUOI_RUST.has(d)) {
    // Ngăn xếp `impl`: nhớ tên kiểu và mức thụt lề mở nó, để biết một `fn`
    // đang nằm trong impl nào. Đóng khi gặp `}` ở đúng mức thụt lề đó.
    const nganXep = []
    dong.forEach((l, i) => {
      const line = i + 1
      const dong_impl = l.match(RUST_IMPL)
      if (dong_impl) {
        nganXep.push({ ten: dong_impl[2], thut: dong_impl[1].length })
        ra.push({ id: dong_impl[2], ten: dong_impl[2], kind: 'impl', line })
        return
      }
      const dongDong = l.match(/^(\s*)\}/)
      if (dongDong && nganXep.length && dongDong[1].length <= nganXep.at(-1).thut) {
        nganXep.pop()
      }
      const mac = l.match(RUST_MACRO)
      if (mac) {
        ra.push({ id: mac[2], ten: mac[2], kind: 'macro', line })
        return
      }
      const m = l.match(RUST_ITEM)
      if (!m) return
      const [, thut, kind, ten] = m
      // `impl` cha chỉ tính khi khối con thụt sâu hơn — tránh gán nhầm một
      // `fn` cấp module cho impl vừa đóng mà regex `}` không bắt được.
      const cha = nganXep.length && thut.length > nganXep.at(-1).thut ? nganXep.at(-1).ten : null
      ra.push({ id: cha ? `${cha}::${ten}` : ten, ten, kind, line })
    })
    return ra
  }

  if (DUOI_TS.has(d)) {
    dong.forEach((l, i) => {
      const m = l.match(TS_ITEM) || l.match(TS_BINDING)
      if (!m) return
      // CHỈ nhận khai báo ở CẤP NGOÀI CÙNG (thụt lề 0) hoặc có `export`.
      // Biến cục bộ trong thân hàm không phải neo tài liệu: `const source` nằm
      // giữa một handler thì cái tên đó không định vị được gì cho người đọc, và
      // đổi tên một biến cục bộ cũng không đáng làm đỏ CI.
      const capNgoai = m[1].length === 0 || /^\s*export\s/.test(l)
      if (!capNgoai) return
      ra.push({ id: m[2], ten: m[2], kind: 'ts', line: i + 1 })
    })
    return ra
  }

  return ra
}

/**
 * Ký hiệu BAO của một dòng: khai báo gần nhất ở trên nó.
 *
 * Dùng cho `--suggest`: đổi `file.rs:123` thành neo nào. Bỏ qua `impl` trần khi
 * ngay dưới nó còn một `fn` gần hơn — neo vào phương thức bao giờ cũng sát
 * nghĩa hơn neo vào cả khối impl.
 */
export function kyHieuBao(kyHieu, line) {
  let tot = null
  for (const k of kyHieu) {
    if (k.line > line) break
    if (k.kind === 'impl' && tot && tot.line >= k.line) continue
    tot = k
  }
  return tot
}
