import { DatabaseSync } from 'node:sqlite'
import crypto from 'node:crypto'

const LOCAL_MEMORY_DOMAIN = 'memory_owner:local'

// Phải khớp `liva-native-core/src/crypto.rs`, và cả ba hằng số dưới đây đã được
// đối chiếu với code Rust chứ không phỏng đoán:
//   HKDF_INFO = b"liva-facts-encryption-v2"  (crypto.rs:18)
//   SALT_LEN  = 16                           (crypto.rs:19)
//   IV / tag  = 16 byte mỗi cái              (crypto.rs:263)
// Lưu ý IV **16 byte**, không phải 12 byte mặc định của GCM: Rust dùng
// `AesGcm<Aes256, U16>` (crypto.rs:10). Phía Node vì thế bắt buộc truyền
// `{ authTagLength: 16 }` kèm IV 16 byte, thiếu là lệch âm thầm.
const HKDF_INFO = 'liva-facts-encryption-v2'
const V2_PREFIX = 'v2:'

// Khoá dẫn xuất theo từng salt, mà salt nằm trong từng bản ghi ⇒ mỗi dòng một
// lần HKDF. Cache theo salt để một lần chạy không dẫn xuất lại cho cùng salt.
const cacheKhoa = new Map()

const danXuatKhoa = (passphrase, salt) => {
  const khoaCache = `${passphrase}|${salt.toString('hex')}`
  const san = cacheKhoa.get(khoaCache)
  if (san) return san
  const key = Buffer.from(
    crypto.hkdfSync('sha256', Buffer.from(passphrase, 'utf8'), salt, Buffer.from(HKDF_INFO, 'utf8'), 32),
  )
  cacheKhoa.set(khoaCache, key)
  return key
}

/**
 * Giải mã một bao `v2:salt:iv:tag:cipher` (hex). Trả `null` khi chuỗi không
 * phải bao v2 — người gọi hiểu đó là plaintext và dùng thẳng.
 *
 * **Fail-closed có chủ đích:** giải mã hỏng thì NÉM, không trả nguyên
 * ciphertext. `crypto.rs:178-187` ghi lại đúng lỗ hổng mà fail-open từng gây ra
 * (ciphertext chảy vào prompt LLM và UI như bộ nhớ thật). Với một bộ kiểm, nuốt
 * lỗi giải mã còn tệ hơn: nó sẽ so chuỗi trên ciphertext và **luôn** báo "không
 * thấy" — đỏ vì lý do sai, đúng cái bẫy hàm này vừa được viết lại để thoát ra.
 */
export const giaiMaBaoV2 = (giaTri, passphrase) => {
  if (typeof giaTri !== 'string' || !giaTri.startsWith(V2_PREFIX)) return null
  const phan = giaTri.split(':')
  if (phan.length !== 5) return null
  const [, saltHex, ivHex, tagHex, cipherHex] = phan
  const salt = Buffer.from(saltHex, 'hex')
  const iv = Buffer.from(ivHex, 'hex')
  const tag = Buffer.from(tagHex, 'hex')
  const cipher = Buffer.from(cipherHex, 'hex')
  if (salt.length !== 16 || iv.length !== 16 || tag.length !== 16) return null

  const decipher = crypto.createDecipheriv('aes-256-gcm', danXuatKhoa(passphrase, salt), iv, {
    authTagLength: 16,
  })
  decipher.setAuthTag(tag)
  return Buffer.concat([decipher.update(cipher), decipher.final()]).toString('utf8')
}

/**
 * Phép kiểm E2E tất định cho bộ nhớ hội thoại, không phải mở lại lệnh tìm kiếm
 * thô. `dbPath` phải là file DB riêng của gateway đang được kiểm.
 *
 * ⚠️ **Vì sao phép so chuỗi KHÔNG còn nằm trong SQL — đo 16/08/2026.** Bản cũ
 * làm `instr(memory.content, ?) > 0` ngay trong truy vấn. Từ khi
 * `vectors_meta.content` được mã hoá (bao `v2:`), điều kiện đó **không bao giờ
 * khớp**: nó tìm plaintext trong một cột ciphertext. Triệu chứng cực dễ đọc
 * nhầm — `e2e-memory.mjs` tụt 6/6 → 4/6, trông y hệt "bộ nhớ vỡ", trong khi
 * kiểm mềm vẫn đạt vì LIVA nhớ đúng thật. Soi DB thì 4 dòng `conversation_turn`
 * nằm đủ, đúng `domain`, đúng `consolidation_status`, giải mã ra đúng nội dung;
 * chỉ bộ kiểm là mù.
 *
 * ⇒ Mọi điều kiện **phạm vi** (type / domain / category / status / liên kết
 * `source_event_ids`) vẫn ở lại SQL — chúng lọc trên cột không mã hoá và chính
 * chúng làm nên sức mạnh của phép kiểm. Chỉ phép so **nội dung** chuyển ra
 * JavaScript, sau khi giải mã. Không nới lỏng điều kiện nào để cho nó xanh.
 *
 * @param {string} dbPath đường dẫn file SQLite của gateway đang kiểm
 * @param {string} marker chuỗi phải xuất hiện trong nội dung lượt hội thoại
 * @param {string} [ownerDomain] scope chủ sở hữu, mặc định `memory_owner:local`
 * @param {string} [passphrase] khoá; mặc định lấy `LIVA_ENCRYPTION_KEY`
 */
export const conversationMemoryContains = (
  dbPath,
  marker,
  ownerDomain = LOCAL_MEMORY_DOMAIN,
  passphrase = process.env.LIVA_ENCRYPTION_KEY ?? '',
) => {
  const db = new DatabaseSync(dbPath, { readOnly: true })
  try {
    const rows = db
      .prepare(`
        SELECT memory.content AS content
        FROM vectors_meta AS memory
        INNER JOIN events AS event ON event.eventId = memory.vec_id
        WHERE memory.type = 'conversation_turn'
          AND memory.domain = ?
          AND event.domain = memory.domain
          AND event.category = memory.category
          AND event.consolidation_status IN ('pending', 'consolidated')
          AND EXISTS (
            SELECT 1
            FROM json_each(memory.source_event_ids)
            WHERE json_each.value = event.eventId
          )
      `)
      .all(ownerDomain)

    for (const row of rows) {
      const noiDung = row?.content
      if (typeof noiDung !== 'string') continue
      // Bao v2 → giải mã; không phải bao v2 → plaintext, dùng thẳng. Giữ cả hai
      // nhánh vì fixture trong `memory-db.test.mjs` chèn plaintext, và bản ghi
      // cũ trước 22/07/2026 cũng chưa mã hoá.
      const ro = noiDung.startsWith(V2_PREFIX) ? giaiMaBaoV2(noiDung, passphrase) : noiDung
      if (typeof ro === 'string' && ro.includes(marker)) return true
    }
    return false
  } finally {
    db.close()
  }
}
