use aes_gcm::aead::consts::U16;
use aes_gcm::{
    AesGcm, Nonce,
    aead::{Aead, KeyInit},
};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::{Digest, Sha256};

type Aes256Gcm16 = AesGcm<aes_gcm::aes::Aes256, U16>;

/// Tiền tố phân biệt định dạng mới (KDF, có salt) với định dạng cũ.
const V2_PREFIX: &str = "v2:";
/// Khoá mã hoá mặc định (công khai trong source). Dùng nó nghĩa là KDF/salt
/// v2 chỉ là bảo mật hình thức — ai đọc được DB cũng tính lại được khoá.
pub const DEFAULT_ENCRYPTION_KEY: &str = "00000000000000000000000000000000";
/// `info` cố định cho HKDF-expand — ràng khoá dẫn xuất vào đúng mục đích này.
const HKDF_INFO: &[u8] = b"liva-facts-encryption-v2";
const SALT_LEN: usize = 16;

/// Mã hoá AES-256-GCM cho dữ liệu nhạy cảm (hiện: `facts.value`).
///
/// # Hai định dạng ciphertext cùng tồn tại
///
/// - **v1** (cũ): `iv:tag:cipher` (hex), khoá = bytes của passphrase pad `0x00`
///   tới 32 byte. Entropy thấp, không salt. **Chỉ còn ĐỌC** để không mất dữ
///   liệu cũ; không sinh mới.
/// - **v2** (22/07/2026): `v2:salt:iv:tag:cipher` (hex), khoá =
///   HKDF-SHA256(passphrase, salt ngẫu nhiên mỗi bản ghi). Salt riêng từng
///   ciphertext ⇒ hai plaintext giống nhau ra ciphertext khác nhau, và khoá
///   được trải đều 32 byte thay vì pad `0x00`.
///
/// `encrypt` luôn sinh **v2**. `decrypt`/`try_decrypt` đọc được cả hai. Dữ liệu
/// v1 cũ được nâng cấp lên v2 bởi [`crate::db::migrate_facts_encryption`] khi
/// khởi động (giải mã bằng khoá v1 rồi mã hoá lại bằng KDF v2 — không mất gì).
#[derive(Clone)]
pub struct EncryptionEngine {
    /// Khoá v1: bytes passphrase pad `0x00` (chỉ để giải mã dữ liệu cũ).
    legacy_key: [u8; 32],
    /// Passphrase gốc, để dẫn xuất khoá v2 qua HKDF với salt mỗi bản ghi.
    passphrase: Vec<u8>,
}

/// Vì sao decrypt thất bại — để tầng gọi phân biệt "chưa mã hoá" (hợp lệ,
/// migration) với "bị sửa đổi" (tấn công) thay vì gộp tất cả thành passthrough.
#[derive(Debug, PartialEq, Eq)]
pub enum DecryptError {
    /// Không phải định dạng `iv:tag:cipher` — nhiều khả năng là plaintext cũ
    /// chưa từng mã hoá. KHÔNG phải lỗi bảo mật.
    NotEncrypted,
    /// Đúng 3 phần nhưng hex/độ dài sai — dữ liệu hỏng.
    BadFormat,
    /// Tag xác thực AES-GCM KHÔNG khớp: ciphertext đã bị sửa đổi hoặc sai khoá.
    /// Đây là ca mà `decrypt` fail-open đang nuốt im lặng.
    AuthFailed,
    /// Giải mã thành công nhưng bytes không phải UTF-8 hợp lệ.
    NotUtf8,
}

/// Kết quả đọc một fact (có thể đã mã hoá) trên đường ĐỌC fail-closed.
///
/// Tách `Ok` khỏi `Locked` để caller (UI/prompt) BIẾT một bản ghi không mở
/// được — thay vì `decrypt_read` cũ gộp cả hai thành `""` (lẫn "giải ra rỗng"
/// với "sai khoá"). `Locked` KHÔNG mang ciphertext ra ngoài, chỉ `reason` thô,
/// để dù caller log/serialize cũng không rò dữ liệu mã hoá.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactRead {
    /// Giải mã (hoặc passthrough plaintext) thành công — `value` dùng được.
    Ok(String),
    /// Không mở được (sai khoá / bị sửa đổi / không UTF-8). Bản gốc CÒN trên
    /// đĩa, đọc lại được khi đúng khoá. `reason` = `"auth_failed"`|`"not_utf8"`.
    Locked { reason: &'static str },
}

impl FactRead {
    /// Có phải bản ghi khoá-chết (không giải mã được) không.
    pub fn is_locked(&self) -> bool {
        matches!(self, FactRead::Locked { .. })
    }

    /// Lấy value, hoặc `""` nếu Locked (KHÔNG bao giờ trả ciphertext).
    pub fn into_value(self) -> String {
        match self {
            FactRead::Ok(s) => s,
            FactRead::Locked { .. } => String::new(),
        }
    }
}

impl EncryptionEngine {
    pub fn new(key_str: &str) -> Self {
        // KDF v2 vô nghĩa nếu passphrase là hằng số công khai. Cảnh báo LỚN,
        // một lần, để không phải "bảo mật ảo" im lặng — nhưng KHÔNG chặn boot
        // (bỏ hẳn default key cần đường thoát cho dữ liệu dev, là quyết định
        // riêng — xem task crypto đã ghim).
        if key_str == DEFAULT_ENCRYPTION_KEY {
            tracing::warn!(
                "⚠️  Đang dùng LIVA_ENCRYPTION_KEY MẶC ĐỊNH (công khai). Mã hoá facts \
                 gần như KHÔNG bảo vệ gì — ai đọc được file DB cũng giải mã được. \
                 Đặt LIVA_ENCRYPTION_KEY thành một khoá bí mật 32 byte cho dữ liệu thật."
            );
        }
        let mut legacy_key = [0u8; 32];
        let bytes = key_str.as_bytes();
        let len = bytes.len().min(32);
        legacy_key[..len].copy_from_slice(&bytes[..len]);
        Self {
            legacy_key,
            passphrase: bytes.to_vec(),
        }
    }

    /// Fingerprint không bí mật để ràng buộc backup với đúng khóa mã hóa.
    ///
    /// Không ghi passphrase vào manifest. 16 byte SHA-256 có domain separation đủ
    /// để phát hiện nhầm khóa với xác suất va chạm không đáng kể; khóa thực vẫn phải
    /// có entropy cao vì mọi fingerprint xác định đều là oracle cho passphrase yếu.
    pub fn key_id(&self) -> String {
        let mut digest = Sha256::new();
        digest.update(b"liva-data-key-id-v1\0");
        digest.update(&self.passphrase);
        hex::encode(&digest.finalize()[..16])
    }

    /// Dẫn xuất khoá 32 byte từ passphrase + salt qua HKDF-SHA256. Không bao
    /// giờ fail với salt/ikm hợp lệ (HKDF-expand chỉ fail khi độ dài ra > 255*32).
    fn derive_key(&self, salt: &[u8]) -> [u8; 32] {
        let hk = Hkdf::<Sha256>::new(Some(salt), &self.passphrase);
        let mut okm = [0u8; 32];
        hk.expand(HKDF_INFO, &mut okm)
            .expect("HKDF-expand 32 byte không thể fail");
        okm
    }

    /// Mã hoá thành định dạng **v2** (KDF + salt mỗi bản ghi). Đường sinh mới
    /// duy nhất; v1 chỉ còn để đọc.
    pub fn encrypt(&self, text: &str) -> Result<String, String> {
        let mut salt = [0u8; SALT_LEN];
        let mut iv = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        rand::rngs::OsRng.fill_bytes(&mut iv);

        let key = self.derive_key(&salt);
        let cipher = Aes256Gcm16::new_from_slice(&key).map_err(|e| e.to_string())?;
        let nonce = Nonce::<U16>::from_slice(&iv);

        let ciphertext_with_tag = cipher
            .encrypt(nonce, text.as_bytes())
            .map_err(|e| e.to_string())?;
        if ciphertext_with_tag.len() < 16 {
            return Err("Ciphertext is too short".to_string());
        }
        let split_idx = ciphertext_with_tag.len() - 16;
        let ciphertext = &ciphertext_with_tag[..split_idx];
        let tag = &ciphertext_with_tag[split_idx..];

        Ok(format!(
            "{}{}:{}:{}:{}",
            V2_PREFIX,
            hex::encode(salt),
            hex::encode(iv),
            hex::encode(tag),
            hex::encode(ciphertext)
        ))
    }

    /// Giải mã fail-OPEN: mọi lỗi trả lại chuỗi đầu vào. Giữ nguyên hợp đồng cũ
    /// để không phá đường migration plaintext của caller. Đọc được CẢ v1 lẫn v2.
    ///
    /// ⚠️ KHÔNG dùng cho đường ĐỌC nạp vào prompt/UI — dùng [`decrypt_read`].
    /// Với sai khoá, hàm này trả lại nguyên **ciphertext** làm "plaintext".
    pub fn decrypt(&self, text: &str) -> String {
        self.try_decrypt(text).unwrap_or_else(|_| text.to_string())
    }

    /// Giải mã cho đường ĐỌC an toàn (facts nạp vào prompt/UI).
    ///
    /// Vá lỗ hổng mất-dữ-liệu do phản biện đối kháng tìm ra (22/07/2026): khi
    /// đổi `LIVA_ENCRYPTION_KEY`, ciphertext của mình sẽ AuthFailed. `decrypt`
    /// fail-open sẽ trả **nguyên ciphertext** làm value — nó chảy vào prompt
    /// LLM và UI như bộ nhớ thật, và nếu bị `set_fact` ghi lại thì lồng 2 lớp,
    /// mất bản gốc VĨNH VIỄN dù khôi phục đúng khoá.
    ///
    /// Phân loại theo lỗi:
    /// - `Ok` → plaintext;
    /// - `NotEncrypted` / `BadFormat` → có thể là **plaintext cũ trông giống
    ///   ciphertext** (vd "12:34:56") → passthrough để KHÔNG mất plaintext hợp lệ;
    /// - `AuthFailed` / `NotUtf8` → **chắc chắn là ciphertext của mình mà không
    ///   mở được** (sai khoá hoặc bị sửa đổi) → trả `""` + cảnh báo, KHÔNG rò
    ///   ciphertext ra ngoài. Dữ liệu gốc vẫn còn trên đĩa, đọc lại được khi
    ///   khoá đúng.
    ///
    /// ⚠️ ĐÁNH ĐỔI CỐ Ý (phản biện vòng 2, 22/07/2026): một plaintext cũ **tình
    /// cờ đúng dạng v1 ciphertext hoàn chỉnh** — `<32 hex>:<32 hex>:<hex chẵn>`
    /// (vd hai mã MD5/fingerprint nối bằng ':') — sẽ QUA được kiểm định dạng,
    /// chạm `open()`, fail AES-GCM → `AuthFailed` → trả `""` thay vì passthrough.
    /// Đây KHÔNG khử được: tại `AuthFailed`, "plaintext trông giống ciphertext"
    /// và "ciphertext thật sai khoá" là **bất khả phân** từ chuỗi byte. Chọn trả
    /// `""` vì ca "ciphertext thật sai khoá" phổ biến hơn HẲN (đổi khoá → MỌI
    /// fact) và rò ciphertext vào prompt/UI là lỗi nặng hơn; hy sinh ca
    /// plaintext-lookalike cực hiếm (facts ngôn ngữ tự nhiên không bao giờ trúng
    /// dạng này). Bản ghi gốc vẫn còn trên đĩa — chỉ mất nếu người dùng thấy `""`
    /// rồi `set_fact` đè lên. Xem test `decrypt_read_plaintext_giong_v1_tra_rong`.
    /// Đọc một fact với PHÂN LOẠI trạng thái (fail-closed, cho đường đọc muốn
    /// biết bản ghi có khoá-chết hay không — vd `get_memory_data` gắn cờ
    /// `locked` cho UI). KHÔNG cảnh báo (caller tự quyết log/gộp).
    ///
    /// Phân loại giống `decrypt_read` nhưng trả về kiểu:
    /// - `Ok` (giải mã được) / `NotEncrypted` / `BadFormat` → `FactRead::Ok`
    ///   (passthrough plaintext cũ để KHÔNG mất bản hợp lệ);
    /// - `AuthFailed` / `NotUtf8` → `FactRead::Locked` (bản gốc còn trên đĩa).
    ///
    /// Đánh đổi cố ý về plaintext-lookalike-v1 giống hệt `decrypt_read` (xem
    /// doc ở đó): quyết định gộp về MỘT chỗ này để không lệch giữa hai đường.
    pub fn read_fact(&self, text: &str) -> FactRead {
        match self.try_decrypt(text) {
            Ok(plain) => FactRead::Ok(plain),
            Err(DecryptError::NotEncrypted) | Err(DecryptError::BadFormat) => {
                FactRead::Ok(text.to_string())
            }
            Err(DecryptError::AuthFailed) => FactRead::Locked {
                reason: "auth_failed",
            },
            Err(DecryptError::NotUtf8) => FactRead::Locked { reason: "not_utf8" },
        }
    }

    pub fn decrypt_read(&self, text: &str) -> String {
        // Wrapper mỏng trên `read_fact` (zero-behavior-change): giữ nguyên
        // hành vi cũ — plaintext/passthrough giữ nguyên, Locked → "" + WARN.
        match self.read_fact(text) {
            FactRead::Ok(s) => s,
            FactRead::Locked { reason } => {
                tracing::warn!(
                    "Không giải mã được một bản ghi ({reason}) — sai LIVA_ENCRYPTION_KEY \
                     hoặc dữ liệu bị sửa đổi. Bỏ qua (không nạp ciphertext vào prompt/UI); \
                     dữ liệu gốc vẫn còn, đọc lại được khi đúng khoá."
                );
                String::new()
            }
        }
    }

    /// Giải mã **fail-CLOSED**: `Err` khi bị sửa đổi (AuthFailed), hỏng, hoặc
    /// chưa mã hoá. Đọc được cả hai định dạng — tự nhận theo tiền tố `v2:`.
    pub fn try_decrypt(&self, text: &str) -> Result<String, DecryptError> {
        match text.strip_prefix(V2_PREFIX) {
            Some(rest) => self.decrypt_v2(rest),
            None => self.decrypt_v1(text),
        }
    }

    /// v2: `salt:iv:tag:cipher`, khoá = HKDF(passphrase, salt).
    fn decrypt_v2(&self, rest: &str) -> Result<String, DecryptError> {
        let parts: Vec<&str> = rest.split(':').collect();
        if parts.len() != 4 {
            return Err(DecryptError::BadFormat);
        }
        let salt = hex::decode(parts[0]).map_err(|_| DecryptError::BadFormat)?;
        let iv = hex::decode(parts[1]).map_err(|_| DecryptError::BadFormat)?;
        let tag = hex::decode(parts[2]).map_err(|_| DecryptError::BadFormat)?;
        let cipher_bytes = hex::decode(parts[3]).map_err(|_| DecryptError::BadFormat)?;
        if salt.len() != SALT_LEN || iv.len() != 16 || tag.len() != 16 {
            return Err(DecryptError::BadFormat);
        }
        let key = self.derive_key(&salt);
        self.open(&key, &iv, &tag, cipher_bytes)
    }

    /// v1 (cũ): `iv:tag:cipher`, khoá = passphrase pad `0x00`. Chỉ để đọc.
    fn decrypt_v1(&self, text: &str) -> Result<String, DecryptError> {
        let parts: Vec<&str> = text.split(':').collect();
        if parts.len() != 3 {
            return Err(DecryptError::NotEncrypted);
        }
        let iv = hex::decode(parts[0]).map_err(|_| DecryptError::BadFormat)?;
        let tag = hex::decode(parts[1]).map_err(|_| DecryptError::BadFormat)?;
        let cipher_bytes = hex::decode(parts[2]).map_err(|_| DecryptError::BadFormat)?;
        if iv.len() != 16 || tag.len() != 16 {
            return Err(DecryptError::BadFormat);
        }
        self.open(&self.legacy_key, &iv, &tag, cipher_bytes)
    }

    /// Bước AES-GCM chung: dựng cipher, ghép tag vào cuối, giải mã. `Err` từ
    /// `decrypt` = tag không khớp = bị sửa đổi hoặc sai khoá.
    fn open(
        &self,
        key: &[u8],
        iv: &[u8],
        tag: &[u8],
        mut cipher_bytes: Vec<u8>,
    ) -> Result<String, DecryptError> {
        let cipher = Aes256Gcm16::new_from_slice(key).map_err(|_| DecryptError::BadFormat)?;
        let nonce = Nonce::<U16>::from_slice(iv);
        cipher_bytes.extend_from_slice(tag);
        let plain = cipher
            .decrypt(nonce, cipher_bytes.as_slice())
            .map_err(|_| DecryptError::AuthFailed)?;
        String::from_utf8(plain).map_err(|_| DecryptError::NotUtf8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_decryption_roundtrip() {
        let key = "00000000000000000000000000000000";
        let engine = EncryptionEngine::new(key);
        let plain = "hello world, this is a secret message!";

        let encrypted = engine.encrypt(plain).unwrap();
        assert_ne!(plain, encrypted);

        // Định dạng mới v2: "v2:salt:iv:tag:cipher".
        assert!(encrypted.starts_with("v2:"), "encrypt phải sinh v2");
        let parts: Vec<&str> = encrypted.split(':').collect();
        assert_eq!(parts.len(), 5, "v2 = v2:salt:iv:tag:cipher");
        assert_eq!(parts[1].len(), 32, "salt 16 byte = 32 hex");
        assert_eq!(parts[2].len(), 32, "iv 16 byte = 32 hex");
        assert_eq!(parts[3].len(), 32, "tag 16 byte = 32 hex");

        let decrypted = engine.decrypt(&encrypted);
        assert_eq!(plain, decrypted);
    }

    #[test]
    fn key_id_on_dinh_khong_lo_khoa_va_phan_biet_khoa() {
        let key_a = "backup-key-a-32-bytes-long-value";
        let a1 = EncryptionEngine::new(key_a);
        let a2 = EncryptionEngine::new(key_a);
        let b = EncryptionEngine::new("backup-key-b-32-bytes-long-value");

        assert_eq!(a1.key_id(), a2.key_id());
        assert_ne!(a1.key_id(), b.key_id());
        assert_eq!(a1.key_id().len(), 32);
        assert!(!a1.key_id().contains(key_a));
    }

    /// Hai plaintext GIỐNG NHAU phải ra ciphertext KHÁC NHAU (salt riêng mỗi
    /// bản ghi) — điều mà kiểu cũ không đảm bảo tốt.
    #[test]
    fn v2_salt_lam_ciphertext_khac_nhau() {
        let engine = EncryptionEngine::new("00000000000000000000000000000000");
        let a = engine.encrypt("cùng một câu").unwrap();
        let b = engine.encrypt("cùng một câu").unwrap();
        assert_ne!(a, b, "salt ngẫu nhiên -> ciphertext phải khác");
        assert_eq!(engine.decrypt(&a), "cùng một câu");
        assert_eq!(engine.decrypt(&b), "cùng một câu");
    }

    /// BACKWARD-COMPAT: dữ liệu v1 (khoá pad 0x00) do bản CŨ ghi phải vẫn giải
    /// mã được — đây là điều kiện để migration không mất dữ liệu. Dựng một
    /// ciphertext v1 thật bằng chính thuật toán cũ.
    #[test]
    fn doc_duoc_du_lieu_v1_cu() {
        use aes_gcm::aead::consts::U16;
        use aes_gcm::{
            AesGcm, Nonce,
            aead::{Aead, KeyInit},
        };
        type G = AesGcm<aes_gcm::aes::Aes256, U16>;

        let key_str = "khoa-cu-cua-toi";
        // Khoá v1: bytes pad 0x00 tới 32.
        let mut raw = [0u8; 32];
        raw[..key_str.len()].copy_from_slice(key_str.as_bytes());
        let iv = [7u8; 16];
        let ct = G::new_from_slice(&raw)
            .unwrap()
            .encrypt(Nonce::<U16>::from_slice(&iv), b"du lieu cu".as_ref())
            .unwrap();
        let (c, tag) = ct.split_at(ct.len() - 16);
        let v1 = format!(
            "{}:{}:{}",
            hex::encode(iv),
            hex::encode(tag),
            hex::encode(c)
        );

        let engine = EncryptionEngine::new(key_str);
        assert_eq!(
            engine.try_decrypt(&v1).unwrap(),
            "du lieu cu",
            "v1 phải đọc được"
        );
        // Và engine mới sinh v2 khác hẳn.
        assert!(engine.encrypt("x").unwrap().starts_with("v2:"));
    }

    #[test]
    fn test_decrypt_plain_fallback() {
        let key = "00000000000000000000000000000000";
        let engine = EncryptionEngine::new(key);

        let plain = "unencrypted plaintext";
        let decrypted = engine.decrypt(plain);
        assert_eq!(plain, decrypted);
    }

    #[test]
    fn test_decrypt_corrupted_fallback() {
        let key = "00000000000000000000000000000000";
        let engine = EncryptionEngine::new(key);

        let encrypted = engine.encrypt("secret").unwrap();
        let corrupted = format!("{}a", encrypted);
        let decrypted = engine.decrypt(&corrupted);
        assert_eq!(corrupted, decrypted);
    }

    // ── try_decrypt: fail-closed, PHÁT HIỆN sửa đổi ──────────────────────────

    #[test]
    fn try_decrypt_roundtrip() {
        let engine = EncryptionEngine::new("00000000000000000000000000000000");
        let plain = "bí mật của Dương — mèo tên Bún";
        let enc = engine.encrypt(plain).unwrap();
        assert_eq!(engine.try_decrypt(&enc).unwrap(), plain);
    }

    /// Đây là điều `decrypt` fail-open đang nuốt: ciphertext bị sửa MỘT byte
    /// phải bị BẮT (AuthFailed), không được coi như plaintext.
    #[test]
    fn try_decrypt_bat_sua_doi() {
        let engine = EncryptionEngine::new("00000000000000000000000000000000");
        let enc = engine.encrypt("chuyển 1000 cho A").unwrap();

        // v2 = "v2:salt:iv:tag:cipher" → lật byte cuối của CIPHER (phần thứ 5).
        let mut parts: Vec<String> = enc.split(':').map(|s| s.to_string()).collect();
        assert_eq!(parts.len(), 5);
        let last = parts[4].pop().unwrap();
        parts[4].push(if last == 'a' { 'b' } else { 'a' });
        let bi_sua = parts.join(":");

        assert_eq!(
            engine.try_decrypt(&bi_sua),
            Err(DecryptError::AuthFailed),
            "ciphertext bi sua doi PHAI bi bat, khong duoc coi la plaintext"
        );
        // Đối chiếu: decrypt cũ nuốt im lặng, trả lại chuỗi bị sửa.
        assert_eq!(
            engine.decrypt(&bi_sua),
            bi_sua,
            "decrypt cu van fail-open (giu nguyen de migration)"
        );
    }

    #[test]
    fn try_decrypt_phan_biet_plaintext_va_hong() {
        let engine = EncryptionEngine::new("00000000000000000000000000000000");
        assert_eq!(
            engine.try_decrypt("chua tung ma hoa"),
            Err(DecryptError::NotEncrypted)
        );
        assert_eq!(
            engine.try_decrypt("khong-phai-hex:zz:yy"),
            Err(DecryptError::BadFormat)
        );
        assert_eq!(
            engine.try_decrypt("aa:bb:cc"),
            Err(DecryptError::BadFormat),
            "iv/tag sai do dai"
        );
    }

    /// VÁ MẤT-DỮ-LIỆU (phản biện 22/07): đọc bằng khoá SAI không được rò
    /// ciphertext ra ngoài — decrypt_read trả "" thay vì nguyên "v2:...".
    /// Nhưng plaintext trông giống ciphertext ("12:34:56") vẫn phải passthrough.
    #[test]
    fn decrypt_read_khong_ro_ciphertext_khi_sai_khoa() {
        let a = EncryptionEngine::new("khoa-A-bi-mat-cua-toi-1234567890");
        let b = EncryptionEngine::new("khoa-B-khac-han-cua-nguoi-khac-9");
        let enc = a.encrypt("số dư 5 triệu").unwrap();

        // Khoá đúng → đọc ra plaintext.
        assert_eq!(a.decrypt_read(&enc), "số dư 5 triệu");
        // Khoá SAI → KHÔNG trả ciphertext (đó là lỗ hổng cũ), trả "".
        assert_eq!(
            b.decrypt_read(&enc),
            "",
            "sai khoá phải trả rỗng, KHÔNG rò ciphertext"
        );
        // Đối chiếu: decrypt fail-open cũ vẫn rò (chứng minh vì sao cần decrypt_read).
        assert_eq!(
            b.decrypt(&enc),
            enc,
            "decrypt fail-open rò ciphertext — đúng lý do có decrypt_read"
        );

        // Plaintext cũ trông giống ciphertext v1 nhưng KHÔNG mở được → passthrough,
        // KHÔNG mất.
        assert_eq!(
            a.decrypt_read("12:34:56"),
            "12:34:56",
            "plaintext-lookalike phải giữ nguyên"
        );
        assert_eq!(a.decrypt_read("ghi chú thường"), "ghi chú thường");
        // Dữ liệu gốc vẫn giải được khi có khoá đúng lại (không mất vĩnh viễn).
        assert_eq!(a.decrypt_read(&enc), "số dư 5 triệu");
    }

    /// ĐÁNH ĐỔI CỐ Ý (phản biện vòng 2, 22/07): plaintext cũ **tình cờ đúng dạng
    /// v1 ciphertext hoàn chỉnh** `<32hex>:<32hex>:<hex chẵn>` KHÔNG passthrough —
    /// nó qua kiểm định dạng, chạm AES-GCM, fail auth → AuthFailed → `""`. Test
    /// này KHOÁ hành vi đó là cố ý: tại AuthFailed, "plaintext trông giống
    /// ciphertext" và "ciphertext thật sai khoá" bất khả phân; ưu tiên KHÔNG rò
    /// ciphertext (ca phổ biến khi đổi khoá) hơn giữ plaintext-lookalike (cực
    /// hiếm). Đừng "sửa" test này thành passthrough — sẽ tái mở lỗ rò ciphertext.
    #[test]
    fn decrypt_read_plaintext_giong_v1_tra_rong() {
        let engine = EncryptionEngine::new("00000000000000000000000000000000");

        // Plaintext thật, nhưng đúng khuôn v1: iv 16B + tag 16B + cipher hex chẵn.
        // (vd hình dung 2 mã MD5 nối bằng ':'.) KHÔNG phải ciphertext của mình.
        let lookalike = format!("{}:{}:{}", "0".repeat(32), "1".repeat(32), "abcdef");

        // Qua kiểm định dạng → chạm open() → AES-GCM fail → AuthFailed.
        assert_eq!(
            engine.try_decrypt(&lookalike),
            Err(DecryptError::AuthFailed),
            "dạng v1 hợp lệ nhưng không mở được PHẢI cho AuthFailed, không phải BadFormat"
        );
        // Hệ quả cố ý: decrypt_read trả "" (không passthrough). Đây là trade-off
        // đã tài liệu hoá, KHÔNG phải bug.
        assert_eq!(
            engine.decrypt_read(&lookalike),
            "",
            "plaintext-lookalike-v1 bị nuốt để KHÔNG rò ciphertext thật sai khoá — cố ý"
        );
        // Đối chiếu: dạng NGẮN không hợp khuôn v1 vẫn passthrough bình thường.
        assert_eq!(engine.decrypt_read("12:34:56"), "12:34:56");
    }

    /// read_fact PHÂN LOẠI được Ok vs Locked (điều decrypt_read "" nuốt mất):
    /// đọc bằng khoá đúng → Ok(value); sai khoá → Locked (KHÔNG Ok("")); plaintext
    /// passthrough → Ok(text). Và decrypt_read vẫn là wrapper trả đúng như cũ.
    #[test]
    fn read_fact_phan_loai_ok_va_locked() {
        let a = EncryptionEngine::new("khoa-A-that-su-bi-mat-1234567890");
        let b = EncryptionEngine::new("khoa-B-khac-han-9876543210zzzzzz");
        let enc = a.encrypt("số dư 5 triệu").unwrap();

        assert_eq!(a.read_fact(&enc), FactRead::Ok("số dư 5 triệu".to_string()));
        assert!(!a.read_fact(&enc).is_locked());

        // Sai khoá → Locked, KHÔNG phải Ok("") (đây là điều typed-status thêm được).
        let locked = b.read_fact(&enc);
        assert!(locked.is_locked(), "sai khoá phải Locked, không phải Ok");
        assert_eq!(
            locked.clone().into_value(),
            "",
            "Locked.into_value() = \"\", không rò ciphertext"
        );
        assert_eq!(
            locked,
            FactRead::Locked {
                reason: "auth_failed"
            }
        );

        // Plaintext cũ passthrough → Ok(text), không Locked.
        assert_eq!(
            a.read_fact("ghi chú thường"),
            FactRead::Ok("ghi chú thường".to_string())
        );
        assert_eq!(
            a.read_fact("12:34:56"),
            FactRead::Ok("12:34:56".to_string())
        );

        // Wrapper decrypt_read vẫn cho đúng chuỗi như trước (zero-behavior-change).
        assert_eq!(a.decrypt_read(&enc), "số dư 5 triệu");
        assert_eq!(b.decrypt_read(&enc), "");
    }

    /// Sai khoá cũng cho AuthFailed — không giải mã nhầm bằng khoá khác.
    #[test]
    fn try_decrypt_sai_khoa() {
        let a = EncryptionEngine::new("00000000000000000000000000000000");
        let b = EncryptionEngine::new("11111111111111111111111111111111");
        let enc = a.encrypt("x").unwrap();
        assert_eq!(b.try_decrypt(&enc), Err(DecryptError::AuthFailed));
    }
}
