//! Đọc thư mục skill dạng `SKILL.md` từ đĩa.
//!
//! Định dạng cố tình **đúng bằng** skill của Claude Code:
//!
//! ```text
//! ---
//! name: ten-skill
//! description: "Khi nào dùng skill này…"
//! ---
//!
//! # Thân bài markdown
//! ```
//!
//! Nhờ vậy 7 skill có sẵn trong `.claude/skills/` của repo này là dữ liệu kiểm
//! thật, không phải fixture tự bịa.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Một skill đã đọc xong từ đĩa, chưa vào DB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedSkill {
    /// Danh tính bền — xem [`doc_skill_id`].
    pub skill_id: String,
    pub name: String,
    pub description: String,
    /// Phần markdown sau front-matter.
    pub body: String,
    /// SHA-256 của `body`, để nhận ra "không đổi gì" mà không so cả thân bài.
    pub body_sha: String,
    pub dir_path: PathBuf,
}

impl LoadedSkill {
    /// Chuỗi để embed / tính BM25.
    ///
    /// Ghép cả ba phần vì mỗi phần một mình đều thiếu: `name` quá ngắn để phân
    /// biệt, `description` là thứ nói "khi nào dùng" (tín hiệu mạnh nhất cho truy
    /// hồi), còn `body` mới chứa từ vựng cụ thể của việc.
    pub fn search_text(&self) -> String {
        format!("{}: {}\n{}", self.name, self.description, self.body)
    }
}

/// Đọc mọi skill dưới một cây thư mục.
///
/// Một thư mục được coi là skill khi nó chứa `SKILL.md`. Quét **không đệ quy vào
/// trong** một skill đã tìm thấy — thư mục con của skill là tài nguyên của nó, và
/// nếu bên trong tình cờ có `SKILL.md` khác thì đó không phải skill độc lập.
///
/// Skill hỏng (thiếu front-matter, thiếu `name`) bị **bỏ qua kèm `warn`**, không
/// làm hỏng cả lượt quét: một file gõ sai không đáng làm cả kho không nạp được.
/// Trả về `Err` chỉ khi bản thân thư mục gốc không đọc được.
pub fn load_skill_tree(root: &Path) -> Result<Vec<LoadedSkill>, String> {
    if !root.is_dir() {
        return Err(format!("không phải thư mục: {}", root.display()));
    }
    let mut ra = Vec::new();
    quet(root, &mut ra, 0);
    ra.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(ra)
}

/// Giới hạn độ sâu quét. Cây skill thật sâu 1–2 tầng; chặn này chỉ để một symlink
/// vòng hoặc thư mục lạ không làm treo lượt quét.
const MAX_DEPTH: usize = 6;

fn quet(dir: &Path, ra: &mut Vec<LoadedSkill>, depth: usize) {
    if depth > MAX_DEPTH {
        tracing::warn!("bỏ qua {} — vượt độ sâu {MAX_DEPTH}", dir.display());
        return;
    }
    if dir.join(super::SKILL_FILE).is_file() {
        match load_skill_dir(dir) {
            Ok(s) => ra.push(s),
            Err(e) => tracing::warn!("bỏ qua skill ở {}: {e}", dir.display()),
        }
        return; // không đi sâu vào trong một skill
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        tracing::warn!("không đọc được thư mục {}", dir.display());
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            quet(&p, ra, depth + 1);
        }
    }
}

/// Đọc một thư mục skill.
pub fn load_skill_dir(dir: &Path) -> Result<LoadedSkill, String> {
    let tep = dir.join(super::SKILL_FILE);
    let raw = std::fs::read_to_string(&tep).map_err(|e| format!("đọc {}: {e}", tep.display()))?;
    // Notepad/PowerShell hay để lại BOM; nó làm `---` ở dòng đầu không khớp.
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(&raw);

    let (fm, body) = tach_front_matter(raw)
        .ok_or_else(|| format!("{} thiếu front-matter `---`", tep.display()))?;

    let name = doc_khoa(&fm, "name")
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("{} thiếu `name:`", tep.display()))?;
    let description = doc_khoa(&fm, "description").unwrap_or_default();

    let body = body.trim().to_string();
    let body_sha = sha_hex(&body);
    let skill_id = doc_skill_id(dir, &name);

    Ok(LoadedSkill {
        skill_id,
        name,
        description,
        body,
        body_sha,
        dir_path: dir.to_path_buf(),
    })
}

/// Đọc danh tính của skill. **THUẦN ĐỌC — không bao giờ ghi file.**
///
/// - `.skill_id` đã có → dùng nguyên. Đây là đường bình thường và là danh tính
///   **bền**: đổi tên thư mục hay sửa `name:` đều không làm mất nó.
/// - Chưa có → id **dẫn xuất tất định** từ `name` (`sha256("name:"+name)`, 32 hex
///   đầu). Tất định nên quét lại không sinh bản ghi trùng, nhưng **không bền qua
///   đổi `name:`** — muốn bền thì gọi [`pin_skill_ids`].
///
/// # Vì sao KHÔNG tự ghi `.skill_id` ở đây
///
/// Bản đầu của hàm này tự sinh UUID rồi ghi file. Hậu quả lộ ra ngay: test đọc 7
/// skill thật trong `.claude/skills/` của repo đã **tạo 7 file mới trong cây
/// nguồn**. Một hàm tên `load_`/`doc_` mà sửa đĩa là bẫy — nó biến mọi lượt quét,
/// kể cả quét chỉ để xem, thành một thay đổi cần review.
///
/// Nên ghim danh tính là một **hành động riêng, có tên riêng**: [`pin_skill_ids`].
pub fn doc_skill_id(dir: &Path, name: &str) -> String {
    let tep = dir.join(super::SKILL_ID_FILE);
    if let Ok(s) = std::fs::read_to_string(&tep) {
        let s = s.trim();
        if !s.is_empty() {
            return s.to_string();
        }
    }
    sha_hex(&format!("name:{name}")).chars().take(32).collect()
}

/// Ghim danh tính bền cho mọi skill dưới `root` bằng cách **ghi** `.skill_id`.
///
/// Hành động có chủ đích, tách khỏi mọi hàm `load_*` (xem [`doc_skill_id`]). Chỉ
/// ghi cho skill **chưa có** file; skill đã có thì không đụng tới.
///
/// Trả `(số ghim mới, số bỏ qua vì đã có)`. Ghi thất bại → `warn` và tính vào số
/// bỏ qua, không làm hỏng cả lượt: một thư mục chỉ-đọc không đáng chặn 20 cái còn
/// lại.
///
/// `.skill_id` **nên được commit** — đó là điều làm danh tính bền qua nhiều máy và
/// nhiều bản clone.
pub fn pin_skill_ids(root: &Path) -> Result<(usize, usize), String> {
    let ds = load_skill_tree(root)?;
    let mut ghim = 0usize;
    let mut bo_qua = 0usize;
    for s in &ds {
        let tep = s.dir_path.join(super::SKILL_ID_FILE);
        if tep.is_file() {
            bo_qua += 1;
            continue;
        }
        let moi = uuid::Uuid::new_v4().to_string();
        match std::fs::write(&tep, format!("{moi}\n")) {
            Ok(()) => {
                tracing::info!("ghim danh tính cho skill '{}': {}", s.name, tep.display());
                ghim += 1;
            }
            Err(e) => {
                tracing::warn!("không ghim được {} ({e}); skill '{}' vẫn dùng id dẫn xuất từ `name`", tep.display(), s.name);
                bo_qua += 1;
            }
        }
    }
    Ok((ghim, bo_qua))
}

/// `pub(crate)` để test của `store` dùng ĐÚNG hàm này thay vì tự tính lại sha —
/// một bản sao trong test sẽ trôi khỏi bản thật rồi che mất lỗi.
pub(crate) fn sha_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

/// Tách khối front-matter khỏi thân bài. `None` nếu không có khối `---`.
fn tach_front_matter(raw: &str) -> Option<(String, String)> {
    let mut lines = raw.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut fm = String::new();
    let mut thay_dong = false;
    for l in lines.by_ref() {
        if l.trim() == "---" {
            thay_dong = true;
            break;
        }
        fm.push_str(l);
        fm.push('\n');
    }
    if !thay_dong {
        return None; // `---` mở mà không đóng: coi như không có front-matter
    }
    Some((fm, lines.collect::<Vec<_>>().join("\n")))
}

/// Đọc một khoá từ front-matter.
///
/// **Đây KHÔNG phải bộ phân tích YAML**, và cố ý không phải: repo không có
/// dependency YAML nào, và front-matter của skill chỉ dùng `key: value`. Xử lý
/// đúng ba thứ gặp thật:
///
/// - giá trị trần: `name: ten-skill`
/// - giá trị trong ngoặc kép, có `\"` bên trong — đúng khuôn
///   `.claude/skills/gitnexus/gitnexus-guide/SKILL.md`
/// - dòng tiếp nối thụt lề (YAML folded): nối vào giá trị trước, cách nhau một
///   khoảng trắng
///
/// Khoá lạ bị bỏ qua. Khuôn YAML phức tạp hơn (block scalar `|`, list) **không**
/// được hỗ trợ — nếu sau này cần, đó là lúc thêm một crate YAML thật, không phải
/// lúc nối thêm vào hàm này.
fn doc_khoa(fm: &str, khoa: &str) -> Option<String> {
    let tien_to = format!("{khoa}:");
    let mut it = fm.lines();
    let dong = it.by_ref().find(|l| l.trim_start().starts_with(&tien_to))?;
    let mut gt = dong.trim_start()[tien_to.len()..].trim().to_string();

    // Dòng tiếp nối: thụt lề và KHÔNG phải một khoá mới.
    for l in it {
        let la_thut_le = l.starts_with(' ') || l.starts_with('\t');
        if !la_thut_le || l.trim().is_empty() {
            break;
        }
        gt.push(' ');
        gt.push_str(l.trim());
    }

    Some(bo_ngoac_kep(&gt))
}

fn bo_ngoac_kep(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        // Bỏ ngoặc ngoài rồi giải `\"` và `\\`.
        let trong = &s[1..s.len() - 1];
        let mut ra = String::with_capacity(trong.len());
        let mut escaped = false;
        for c in trong.chars() {
            if escaped {
                ra.push(c);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else {
                ra.push(c);
            }
        }
        return ra;
    }
    if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
        return s[1..s.len() - 1].to_string();
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tam(ten: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "liva-skill-test-{}-{}-{ten}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn viet_skill(dir: &Path, noi_dung: &str) {
        std::fs::write(dir.join(super::super::SKILL_FILE), noi_dung).unwrap();
    }

    #[test]
    fn doc_duoc_khuon_chuan() {
        let d = tam("chuan");
        viet_skill(
            &d,
            "---\nname: viec-abc\ndescription: Dùng khi cần abc\n---\n\n# Thân\n\nnội dung\n",
        );
        let s = load_skill_dir(&d).expect("phải đọc được");
        assert_eq!(s.name, "viec-abc");
        assert_eq!(s.description, "Dùng khi cần abc");
        assert!(s.body.starts_with("# Thân"));
        assert_eq!(s.body_sha.len(), 64, "sha256 hex");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Đúng khuôn `description` của skill gitnexus có sẵn trong repo: chuỗi trong
    /// ngoặc kép, có `\"` lồng bên trong.
    #[test]
    fn doc_duoc_chuoi_ngoac_kep_co_escape() {
        let d = tam("quoted");
        viet_skill(
            &d,
            "---\nname: g\ndescription: \"Dùng khi hỏi \\\"cái gì\\\" về X\"\n---\nthân\n",
        );
        let s = load_skill_dir(&d).unwrap();
        assert_eq!(s.description, "Dùng khi hỏi \"cái gì\" về X");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn doc_duoc_dong_tiep_noi_thut_le() {
        let d = tam("folded");
        viet_skill(
            &d,
            "---\nname: g\ndescription: dòng một\n  dòng hai\n  dòng ba\n---\nthân\n",
        );
        let s = load_skill_dir(&d).unwrap();
        assert_eq!(s.description, "dòng một dòng hai dòng ba");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn bo_bom_utf8() {
        let d = tam("bom");
        viet_skill(&d, "\u{feff}---\nname: g\ndescription: d\n---\nthân\n");
        assert_eq!(load_skill_dir(&d).unwrap().name, "g");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn thieu_front_matter_hoac_name_thi_bao_loi() {
        let d = tam("khong-fm");
        viet_skill(&d, "# chỉ có markdown\n");
        assert!(load_skill_dir(&d).is_err());

        // `---` mở mà không đóng cũng phải là lỗi, không được nuốt cả file làm fm.
        viet_skill(&d, "---\nname: g\nkhong dong\n");
        assert!(load_skill_dir(&d).is_err());

        viet_skill(&d, "---\ndescription: co mo ta nhung khong ten\n---\nthân\n");
        let e = load_skill_dir(&d).expect_err("thiếu name phải lỗi");
        assert!(e.contains("name"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// `load_*` phải THUẦN ĐỌC. Bản đầu của loader tự ghi `.skill_id`, và test đọc
    /// skill thật của repo đã tạo 7 file mới trong cây nguồn — đây là hồi quy cho
    /// đúng chuyện đó.
    #[test]
    fn load_khong_bao_gio_ghi_skill_id() {
        let d = tam("thuan-doc");
        viet_skill(&d, "---\nname: g\ndescription: d\n---\nthân\n");
        let s1 = load_skill_dir(&d).unwrap();
        assert!(
            !d.join(super::super::SKILL_ID_FILE).exists(),
            "load KHÔNG được tạo .skill_id"
        );
        // Và id dẫn xuất phải TẤT ĐỊNH, để quét lại không sinh bản ghi trùng.
        assert_eq!(s1.skill_id, load_skill_dir(&d).unwrap().skill_id);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Sau khi ghim tường minh, danh tính phải BỀN: đổi cả tên thư mục lẫn `name:`
    /// vẫn ra đúng một id. Đây là cả lý do tồn tại của `.skill_id`.
    #[test]
    fn pin_roi_thi_id_ben_qua_doi_ten_va_doi_name() {
        let d = tam("ben");
        viet_skill(&d, "---\nname: ten-cu\ndescription: d\n---\nthân\n");

        let (ghim, bo_qua) = pin_skill_ids(&d).expect("ghim được");
        assert_eq!((ghim, bo_qua), (1, 0));
        assert!(d.join(super::super::SKILL_ID_FILE).is_file());
        let id1 = load_skill_dir(&d).unwrap().skill_id;

        // Ghim lần hai không được đụng file đã có.
        assert_eq!(pin_skill_ids(&d).unwrap(), (0, 1), "đã ghim thì bỏ qua");
        assert_eq!(load_skill_dir(&d).unwrap().skill_id, id1);

        // Đổi `name:` — id phải giữ nguyên.
        viet_skill(&d, "---\nname: ten-hoan-toan-khac\ndescription: d\n---\nthân\n");
        assert_eq!(load_skill_dir(&d).unwrap().skill_id, id1, "đổi name không đổi id");

        // Đổi tên thư mục — id phải giữ nguyên.
        let d2 = d.with_extension("da-doi-ten");
        std::fs::rename(&d, &d2).unwrap();
        assert_eq!(load_skill_dir(&d2).unwrap().skill_id, id1, "đổi thư mục không đổi id");
        let _ = std::fs::remove_dir_all(&d2);
    }

    /// Ngược lại: CHƯA ghim thì id dẫn xuất từ `name`, nên đổi `name:` LÀ đổi danh
    /// tính. Ghi rõ thành test để không ai tưởng id dẫn xuất cũng bền.
    #[test]
    fn chua_pin_thi_doi_name_la_doi_danh_tinh() {
        let d = tam("chua-pin");
        viet_skill(&d, "---\nname: a\ndescription: d\n---\nthân\n");
        let id_a = load_skill_dir(&d).unwrap().skill_id;
        viet_skill(&d, "---\nname: b\ndescription: d\n---\nthân\n");
        let id_b = load_skill_dir(&d).unwrap().skill_id;
        assert_ne!(id_a, id_b, "đây là giới hạn CÓ Ý của id dẫn xuất");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn quet_cay_bo_qua_skill_hong_va_khong_di_sau_vao_skill() {
        let goc = tam("cay");
        let a = goc.join("a");
        let b = goc.join("nested/b");
        let hong = goc.join("hong");
        let long = a.join("tai-nguyen"); // thư mục con CỦA skill a
        for p in [&a, &b, &hong, &long] {
            std::fs::create_dir_all(p).unwrap();
        }
        viet_skill(&a, "---\nname: aaa\ndescription: d\n---\nthân a\n");
        viet_skill(&b, "---\nname: bbb\ndescription: d\n---\nthân b\n");
        viet_skill(&hong, "khong co front matter\n");
        // SKILL.md lồng trong skill a: KHÔNG được tính là skill riêng.
        viet_skill(&long, "---\nname: khong-duoc-tinh\ndescription: d\n---\nx\n");

        let ds = load_skill_tree(&goc).expect("quét được");
        let ten: Vec<&str> = ds.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            ten,
            vec!["aaa", "bbb"],
            "chỉ 2 skill hợp lệ, không tính skill lồng, bỏ qua skill hỏng"
        );
        let _ = std::fs::remove_dir_all(&goc);
    }

    #[test]
    fn khong_phai_thu_muc_thi_bao_loi() {
        assert!(load_skill_tree(Path::new("khong-ton-tai-dau-xxx")).is_err());
    }

    /// Kho phải đọc được **skill thật trong repo này**, không chỉ fixture tự viết.
    #[test]
    fn doc_duoc_skill_that_cua_repo() {
        let goc = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.claude/skills");
        if !goc.is_dir() {
            eprintln!("!!! BỎ QUA: không thấy {} — không kiểm được ca dữ liệu thật", goc.display());
            return;
        }
        let ds = load_skill_tree(&goc).expect("quét được cây skill của repo");
        assert!(
            ds.len() >= 5,
            "repo có sẵn nhiều skill gitnexus; đọc được {} cái",
            ds.len()
        );
        assert!(
            ds.iter().all(|s| !s.name.is_empty() && !s.body.is_empty()),
            "mọi skill đọc được phải có name và thân bài"
        );
        assert!(
            ds.iter().any(|s| !s.description.is_empty()),
            "ít nhất một skill phải có description đọc được"
        );
    }
}
