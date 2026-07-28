//! Tải model lần đầu — **trong ứng dụng**, không cần Node, Rust hay Git.
//!
//! # Vì sao module này tồn tại
//!
//! Weight bị gitignore và nặng 2,28 GB ở mức tối thiểu (3,65 GB nếu lấy cả phần
//! tuỳ chọn), nên không thể nhét vào bộ cài. Trước module này, cách duy nhất để có model là `npm run setup:models`
//! — một script Node chạy từ cây mã nguồn. Người cài bản `.exe` không có cây mã
//! nguồn, không có `scripts/`, và gần như chắc chắn không có Node. Với họ,
//! "thiếu model" không phải trạng thái sửa được: LIVA khởi động, nhận lệnh, rồi
//! im lặng không nghe không nói không nhớ.
//!
//! Danh sách model KHÔNG được chép lại ở đây. Nó nằm ở `data/models-manifest.json`
//! và được `scripts/models.mjs` đọc cùng — hai bản danh sách song song thì sớm
//! muộn cũng lệch, và bên lệch là bên người dùng chạy.
//!
//! # Ranh giới
//!
//! - **Không chặn runtime.** Tải bằng `reqwest` bất đồng bộ và `tokio::fs`;
//!   không có `std::fs::write` nào trên đường tải, không `block_on`.
//! - **Nối lại được.** Ghi vào `<đích>.dangtai` rồi mới `rename`, và gửi
//!   `Range:` khi chạy lại. Đối tượng dùng nó tải vài GB qua mạng gia đình; rớt
//!   ở phút thứ 20 mà phải tải lại từ đầu thì tính năng này coi như không có.
//! - **Không tự ý tải.** Gọi từ lệnh `setup:fetch`, do người dùng bấm.
//! - **Không tin kích thước.** Mỗi file bị băm SHA-256 theo dòng và chỉ được
//!   `rename` vào đường dẫn thật khi hash khớp manifest. Kích thước không phải
//!   bằng chứng: 28/07/2026 có bốn file trên máy dev đúng từng byte mà nội dung
//!   khác nguồn — ba trong số đó khớp tới từng byte một.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Tên file manifest, tính từ gốc tài nguyên. Đóng gói kèm bộ cài.
pub const MANIFEST_REL: &str = "data/models-manifest.json";

/// Đuôi file tạm khi đang tải. Trùng với `scripts/models.mjs` để hai bên nối
/// tiếp được phần tải dở của nhau.
const DUOI_TAM: &str = ".dangtai";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Group {
    pub name: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub broken: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFile {
    pub group: String,
    pub profile: String,
    #[serde(default)]
    pub llm: bool,
    pub dest: String,
    #[serde(default)]
    pub url: Option<String>,
    pub bytes: u64,
    #[serde(default)]
    pub exact_size: bool,
    #[serde(default)]
    pub manual: Option<String>,
    /// SHA-256 hex của nội dung. **Bắt buộc** khi có `url` — xem [`parse_manifest`].
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Manifest {
    pub groups: std::collections::HashMap<String, Group>,
    pub files: Vec<ModelFile>,
}

/// Ba trạng thái, cố ý KHÔNG gộp "lệch kích thước" vào "hỏng" — cùng lý lẽ với
/// `soKichThuoc` trong `scripts/models.mjs`: `bytes` phần lớn là số đo tham
/// chiếu, nên báo đỏ khi nguồn ra bản mới hợp lệ là báo oan, và một cảnh báo hay
/// báo oan thì vài lần là người ta bỏ qua luôn cả cái đúng.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileState {
    /// Có, đúng kích thước tham chiếu.
    Ok,
    /// Không có file.
    Missing,
    /// Có nhưng sai kích thước, và nguồn đã được đối chiếu ⇒ hỏng thật.
    Corrupt,
    /// Có nhưng lệch kích thước tham chiếu — cảnh báo, không phải lỗi.
    Drifted,
}

impl FileState {
    /// Có cần tải (lại) không. `Drifted` chỉ tải lại khi người dùng ép.
    pub fn needs_download(self, force: bool) -> bool {
        match self {
            FileState::Missing | FileState::Corrupt => true,
            FileState::Drifted => force,
            FileState::Ok => false,
        }
    }
}

/// Trạng thái một file, tính từ kích thước thực tế trên đĩa.
///
/// Thuần (nhận `actual`) để test được toàn bộ bảng quyết định mà không phải tạo
/// file thật cho từng nhánh.
pub fn file_state(actual: Option<u64>, f: &ModelFile) -> FileState {
    match actual {
        None => FileState::Missing,
        Some(n) if n == f.bytes => FileState::Ok,
        Some(_) if f.exact_size => FileState::Corrupt,
        Some(_) => FileState::Drifted,
    }
}

/// Đường dẫn thật của một file model.
///
/// Hai gốc khác nhau là có chủ đích: GGUF thường nằm ở ổ khác (vài GB, người
/// dùng hay để riêng), còn ONNX đi kèm ứng dụng.
pub fn target_path(f: &ModelFile, llm_dir: &Path, resource_root: &Path) -> PathBuf {
    if f.llm {
        llm_dir.join(&f.dest)
    } else {
        resource_root.join(&f.dest)
    }
}

/// Lọc theo profile: `minimal` là bộ tối thiểu để LIVA dùng được, `full` là tất cả.
pub fn for_profile<'a>(m: &'a Manifest, profile: &str) -> Vec<&'a ModelFile> {
    m.files
        .iter()
        .filter(|f| profile == "full" || f.profile == "minimal")
        .collect()
}

/// Một file trong báo cáo trạng thái.
#[derive(Debug, Clone, Serialize)]
pub struct FileReport {
    pub group: String,
    pub dest: String,
    pub state: FileState,
    pub bytes: u64,
    /// `false` khi không có nguồn tải công khai (tự train / tự export).
    pub downloadable: bool,
    pub manual: Option<String>,
}

/// Báo cáo cho UI: nhóm nào sẵn sàng, thiếu bao nhiêu byte, có chặn dùng không.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub profile: String,
    pub llm_dir: String,
    pub resource_root: String,
    pub groups: Vec<GroupReport>,
    pub missing: Vec<FileReport>,
    pub missing_bytes: u64,
    /// `true` khi còn thiếu file thuộc nhóm BẮT BUỘC ⇒ LIVA chưa dùng được.
    pub blocking: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupReport {
    pub key: String,
    pub name: String,
    pub required: bool,
    pub ready: bool,
    pub broken: String,
    pub note: String,
}

/// Đọc manifest đã đóng gói kèm ứng dụng.
pub fn load_manifest() -> Result<Manifest, String> {
    let path = crate::resolve_resource_path(MANIFEST_REL);
    let raw = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "không đọc được danh sách model tại {}: {e}. File này phải được đóng gói \
             kèm ứng dụng (bundle.resources trong tauri.conf.json)",
            path.display()
        )
    })?;
    parse_manifest(&raw)
}

/// Đúng 64 chữ số hex, không hoa không thường lẫn lộn cũng được.
pub fn la_hex_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// SHA-256 của một khối byte, dạng hex.
///
/// Chỉ dùng trong test: đường tải thật băm theo dòng (`Sha256` cập nhật từng
/// chunk) để không phải nạp 1 GB vào RAM, nên nó không gọi hàm này.
#[cfg(test)]
fn hex_sha256(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

/// So hash mong đợi với nội dung thật.
///
/// Chỉ dùng trong test — nhưng nó và đường tải thật **dùng chung** [`loi_hash`],
/// nên phép so trong test không thể trôi khỏi thông báo mà người dùng nhận.
#[cfg(test)]
fn kiem_hash(mong_doi: &str, data: &[u8]) -> Result<(), String> {
    let thuc = hex_sha256(data);
    if thuc.eq_ignore_ascii_case(mong_doi) {
        return Ok(());
    }
    Err(loi_hash(mong_doi, &thuc))
}

/// Thông báo phải nói cả hai phía: người nhận cần phân biệt "tải dở/hỏng mạng"
/// với "file mình vừa nhận KHÔNG phải file dự án công bố".
fn loi_hash(mong_doi: &str, thuc: &str) -> String {
    format!(
        "SHA-256 KHÔNG khớp — file nhận được không phải file LIVA công bố. \
         Mong đợi {mong_doi}, nhận {thuc}. File đã bị xoá; nếu chạy lại vẫn lệch, \
         đừng dùng nó."
    )
}

/// Tách khỏi I/O để test được bằng chuỗi.
///
/// **Fail closed**: entry có `url` mà thiếu `sha256`, hoặc `sha256` không phải 64
/// chữ số hex, thì cả manifest bị từ chối. Chấp nhận một entry không hash nghĩa là
/// mở đúng một khe cho thứ ta đang chặn — và khe đó sẽ nằm ở file mà người thêm
/// nó vội nhất.
pub fn parse_manifest(raw: &str) -> Result<Manifest, String> {
    let m: Manifest =
        serde_json::from_str(raw).map_err(|e| format!("danh sách model hỏng: {e}"))?;
    for f in &m.files {
        if !m.groups.contains_key(&f.group) {
            return Err(format!(
                "danh sách model hỏng: {} thuộc nhóm \"{}\" chưa khai báo",
                f.dest, f.group
            ));
        }
        if f.url.is_some() {
            match f.sha256.as_deref() {
                None => {
                    return Err(format!(
                        "danh sách model hỏng: {} có url nhưng THIẾU sha256 — \
                         không tải khi chưa có gì để đối chiếu",
                        f.dest
                    ));
                }
                Some(h) if !la_hex_sha256(h) => {
                    return Err(format!(
                        "danh sách model hỏng: {} có sha256 sai định dạng \
                         (cần đúng 64 chữ số hex, nhận {} ký tự)",
                        f.dest,
                        h.len()
                    ));
                }
                Some(_) => {}
            }
        }
    }
    Ok(m)
}

/// Kích thước file trên đĩa, `None` khi không có.
fn kich_thuoc(p: &Path) -> Option<u64> {
    std::fs::metadata(p).ok().map(|m| m.len())
}

/// Soi đĩa và dựng báo cáo. Đây là thứ `setup:status` trả về.
pub fn status(m: &Manifest, profile: &str, llm_dir: &Path, resource_root: &Path) -> Status {
    let ds = for_profile(m, profile);

    let mut missing = Vec::new();
    let mut missing_bytes = 0u64;
    let mut blocking = false;

    let mut theo_nhom: std::collections::HashMap<&str, bool> = std::collections::HashMap::new();
    for f in &ds {
        let path = target_path(f, llm_dir, resource_root);
        let st = file_state(kich_thuoc(&path), f);
        let ready = st == FileState::Ok || st == FileState::Drifted;
        let e = theo_nhom.entry(f.group.as_str()).or_insert(true);
        *e = *e && ready;
        if !ready {
            missing_bytes += f.bytes;
            missing.push(FileReport {
                group: f.group.clone(),
                dest: f.dest.clone(),
                state: st,
                bytes: f.bytes,
                downloadable: f.url.is_some(),
                manual: f.manual.clone(),
            });
            if m.groups.get(&f.group).map(|g| g.required).unwrap_or(false) {
                blocking = true;
            }
        }
    }

    let mut groups: Vec<GroupReport> = theo_nhom
        .into_iter()
        .map(|(k, ready)| {
            let g = m.groups.get(k);
            GroupReport {
                key: k.to_string(),
                name: g.map(|g| g.name.clone()).unwrap_or_else(|| k.to_string()),
                required: g.map(|g| g.required).unwrap_or(false),
                ready,
                broken: g.map(|g| g.broken.clone()).unwrap_or_default(),
                note: g.map(|g| g.note.clone()).unwrap_or_default(),
            }
        })
        .collect();
    // Thứ tự ổn định: bắt buộc-chưa-xong lên đầu, rồi theo tên. UI đọc thẳng
    // mảng này nên nó không được đổi thứ tự giữa hai lần gọi.
    groups.sort_by(|a, b| {
        (!a.required, a.ready, a.key.clone()).cmp(&(!b.required, b.ready, b.key.clone()))
    });

    Status {
        profile: profile.to_string(),
        llm_dir: llm_dir.display().to_string(),
        resource_root: resource_root.display().to_string(),
        groups,
        missing,
        missing_bytes,
        blocking,
    }
}

/// Tiến độ một lần tải, đủ để UI vẽ mà không phải tự suy ra gì.
#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    /// File thứ mấy trên tổng số (1-based).
    pub index: usize,
    pub total_files: usize,
    pub dest: String,
    pub downloaded: u64,
    /// `0` khi máy chủ không cho biết tổng.
    pub total: u64,
    /// Byte đã tải của TOÀN bộ lượt, để vẽ thanh tổng.
    pub overall_downloaded: u64,
    pub overall_total: u64,
}

/// Kết quả một lượt tải.
#[derive(Debug, Clone, Serialize)]
pub struct FetchSummary {
    pub downloaded: usize,
    pub skipped_manual: Vec<String>,
    pub failed: Vec<String>,
}

/// Tải một file, có RESUME và RETRY.
///
/// Không dùng `bytes()` gộp cả file vào RAM: file lớn nhất ở đây 1,03 GB (bật
/// Parakeet thì 2,4 GB) — nạp trọn vào bộ nhớ là chết đúng ở file quan trọng nhất.
async fn tai_mot_file(
    client: &reqwest::Client,
    url: &str,
    dich: &Path,
    sha256: &str,
    so_lan: u32,
    mut bao_tien_do: impl FnMut(u64, u64),
) -> Result<u64, String> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncWriteExt;

    // `<tên đầy đủ>.dangtai`, không phải `with_extension` — với `foo.onnx.data`
    // thì `with_extension` thay phần sau dấu chấm CUỐI và cho `foo.onnx.dangtai`,
    // đè lên file tạm của `foo.onnx`. Trùng tên tạm giữa hai file thật là cách
    // êm nhất để trộn nội dung hai model vào nhau.
    let tam = match dich.file_name() {
        Some(n) => dich.with_file_name(format!("{}{DUOI_TAM}", n.to_string_lossy())),
        None => return Err(format!("đường dẫn đích không hợp lệ: {}", dich.display())),
    };
    if let Some(cha) = dich.parent() {
        tokio::fs::create_dir_all(cha)
            .await
            .map_err(|e| format!("không tạo được thư mục {}: {e}", cha.display()))?;
    }

    let mut loi_cuoi = String::new();
    for lan in 1..=so_lan {
        let da_co = tokio::fs::metadata(&tam)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        let mut req = client.get(url);
        if da_co > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={da_co}-"));
        }

        // `Some(_)` = hash lệch ⇒ phải XOÁ file tạm trước khi thử lại. Giữ lại một
        // file tạm đã sai nội dung rồi `Range:` ghi tiếp lên nó là cách chắc chắn
        // nhất để mọi lần thử sau đều lệch vì đúng cái lý do cũ.
        let mut hash_lech: Option<String> = None;

        let ket_qua: Result<u64, String> = async {
            let res = req.send().await.map_err(|e| e.to_string())?;

            // 416 = "không còn byte nào sau vị trí đó" ⇒ phần tạm đã đủ. Vẫn phải
            // băm rồi mới nhận: "đủ số byte" chưa bao giờ là bằng chứng về nội dung.
            if res.status().as_u16() == 416 && da_co > 0 {
                let mut h = Sha256::new();
                doc_vao_hash(&tam, &mut h).await?;
                let thuc = hex::encode(h.finalize());
                if !thuc.eq_ignore_ascii_case(sha256) {
                    hash_lech = Some(loi_hash(sha256, &thuc));
                    return Err(hash_lech.clone().unwrap_or_default());
                }
                tokio::fs::rename(&tam, dich)
                    .await
                    .map_err(|e| e.to_string())?;
                return Ok(da_co);
            }
            if !res.status().is_success() {
                return Err(format!("HTTP {}", res.status()));
            }

            // 206 = máy chủ chấp nhận Range ⇒ ghi tiếp. 200 = không ⇒ làm lại từ đầu.
            let tiep_tuc = res.status().as_u16() == 206 && da_co > 0;
            let con_lai = res.content_length().unwrap_or(0);
            let tong = if tiep_tuc { da_co + con_lai } else { con_lai };

            // Băm TOÀN BỘ file, kể cả phần đã tải từ lần trước — nếu không, một
            // lượt tải nối tiếp sẽ chỉ băm phần đuôi và cổng hash mất tác dụng
            // đúng vào lúc nó cần nhất.
            let mut h = Sha256::new();
            if tiep_tuc {
                doc_vao_hash(&tam, &mut h).await?;
            }

            let mut f = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(tiep_tuc)
                .truncate(!tiep_tuc)
                .open(&tam)
                .await
                .map_err(|e| format!("không mở được {}: {e}", tam.display()))?;

            let mut da_ghi = if tiep_tuc { da_co } else { 0 };
            let mut res = res;
            while let Some(chunk) = res.chunk().await.map_err(|e| e.to_string())? {
                h.update(&chunk);
                f.write_all(&chunk).await.map_err(|e| e.to_string())?;
                da_ghi += chunk.len() as u64;
                bao_tien_do(da_ghi, tong);
            }
            f.flush().await.map_err(|e| e.to_string())?;
            drop(f);

            // Băm TRƯỚC khi đổi tên: file ở đường dẫn thật là file các bộ nạp
            // C++/ONNX sẽ mở, nên không có gì chưa kiểm được phép tới đó.
            let thuc = hex::encode(h.finalize());
            if !thuc.eq_ignore_ascii_case(sha256) {
                hash_lech = Some(loi_hash(sha256, &thuc));
                return Err(hash_lech.clone().unwrap_or_default());
            }

            tokio::fs::rename(&tam, dich)
                .await
                .map_err(|e| format!("không đổi tên được sang {}: {e}", dich.display()))?;
            Ok(da_ghi)
        }
        .await;

        match ket_qua {
            Ok(n) => return Ok(n),
            Err(e) => {
                loi_cuoi = e;
                if hash_lech.is_some() {
                    let _ = tokio::fs::remove_file(&tam).await;
                }
                // Lỗi mạng thì GIỮ `.dangtai` để lần sau nối tiếp.
                if lan < so_lan {
                    tokio::time::sleep(std::time::Duration::from_secs(2 * lan as u64)).await;
                }
            }
        }
    }
    Err(loi_cuoi)
}

/// Nạp nội dung `p` vào hasher theo khối 1 MiB.
///
/// Không `read_to_end`: phần đã tải dở có thể là 1 GB, và nạp trọn vào RAM ở đây
/// sẽ giết đúng lượt tải mà tính năng nối-tiếp sinh ra để cứu.
async fn doc_vao_hash(p: &Path, h: &mut sha2::Sha256) -> Result<(), String> {
    use sha2::Digest;
    use tokio::io::AsyncReadExt;

    let mut f = tokio::fs::File::open(p)
        .await
        .map_err(|e| format!("không đọc lại được {}: {e}", p.display()))?;
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(())
}

/// Tải mọi file còn thiếu của `profile`.
///
/// `bao_tien_do` được gọi rất nhiều lần; người gọi tự giảm tần suất trước khi
/// đẩy ra socket.
pub async fn fetch_missing(
    m: &Manifest,
    profile: &str,
    llm_dir: &Path,
    resource_root: &Path,
    force: bool,
    mut bao_tien_do: impl FnMut(Progress),
) -> FetchSummary {
    let ds = for_profile(m, profile);
    let mut can_tai: Vec<&ModelFile> = Vec::new();
    let mut skipped_manual = Vec::new();

    for f in ds {
        let path = target_path(f, llm_dir, resource_root);
        if !file_state(kich_thuoc(&path), f).needs_download(force) {
            continue;
        }
        match f.url {
            Some(_) => can_tai.push(f),
            // Không có nguồn công khai: báo thẳng thay vì giả vờ làm được.
            None => skipped_manual.push(f.dest.clone()),
        }
    }

    let overall_total: u64 = can_tai.iter().map(|f| f.bytes).sum();
    let mut overall_done: u64 = 0;
    let mut downloaded = 0usize;
    let mut failed = Vec::new();

    let client = reqwest::Client::builder()
        // Không đặt timeout tổng: một file 1 GB trên mạng chậm là bình thường,
        // và timeout tổng sẽ giết đúng những lượt tải cần nhất. Chỉ chặn ở khâu
        // kết nối, nơi treo thật sự có nghĩa là hỏng.
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    let tong_file = can_tai.len();
    for (i, f) in can_tai.iter().enumerate() {
        let dich = target_path(f, llm_dir, resource_root);
        let url = f.url.as_deref().unwrap_or_default();
        let da_xong_truoc = overall_done;
        let dest = f.dest.clone();

        // `parse_manifest` đã bảo đảm entry có url thì có sha256 hợp lệ, nên
        // nhánh rỗng dưới đây không thể xảy ra với manifest đã qua cổng — giữ
        // `unwrap_or_default` để nếu ai đó dựng `Manifest` bằng tay mà bỏ hash
        // thì kết quả là "hash rỗng ⇒ mọi file đều lệch ⇒ từ chối", chứ không
        // phải "không có hash ⇒ chấp nhận tất".
        let sha = f.sha256.clone().unwrap_or_default();
        let kq = tai_mot_file(&client, url, &dich, &sha, 3, |da, tong| {
            bao_tien_do(Progress {
                index: i + 1,
                total_files: tong_file,
                dest: dest.clone(),
                downloaded: da,
                total: tong,
                overall_downloaded: da_xong_truoc + da,
                overall_total,
            });
        })
        .await;

        match kq {
            Ok(n) => {
                downloaded += 1;
                overall_done += n;
            }
            Err(e) => {
                overall_done += f.bytes;
                failed.push(format!("{}: {e}", f.dest));
            }
        }
    }

    FetchSummary {
        downloaded,
        skipped_manual,
        failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(dest: &str, bytes: u64, exact: bool, llm: bool) -> ModelFile {
        ModelFile {
            group: "stt".into(),
            profile: "minimal".into(),
            llm,
            dest: dest.into(),
            url: Some("https://x/y".into()),
            bytes,
            exact_size: exact,
            manual: None,
            sha256: Some(hex_sha256(dest.as_bytes())),
        }
    }

    /// Bảng quyết định đầy đủ — đây là chỗ dễ viết sai nhất và cũng là chỗ quyết
    /// định người dùng có phải tải lại 1 GB vô ích hay không.
    #[test]
    fn trang_thai_file_phan_biet_hong_va_lech() {
        let chinh_xac = f("a", 100, true, false);
        let tham_chieu = f("b", 100, false, false);

        assert_eq!(file_state(None, &chinh_xac), FileState::Missing);
        assert_eq!(file_state(Some(100), &chinh_xac), FileState::Ok);
        assert_eq!(
            file_state(Some(99), &chinh_xac),
            FileState::Corrupt,
            "đã đối chiếu content-length thì lệch = hỏng thật"
        );
        assert_eq!(
            file_state(Some(99), &tham_chieu),
            FileState::Drifted,
            "kích thước chỉ là tham chiếu thì lệch KHÔNG được coi là hỏng"
        );
    }

    #[test]
    fn lech_kich_thuoc_khong_tai_lai_tru_khi_ep() {
        assert!(!FileState::Drifted.needs_download(false));
        assert!(FileState::Drifted.needs_download(true));
        assert!(FileState::Missing.needs_download(false));
        assert!(FileState::Corrupt.needs_download(false));
        assert!(!FileState::Ok.needs_download(true));
    }

    #[test]
    fn gguf_di_vao_thu_muc_llm_con_onnx_di_theo_tai_nguyen() {
        let llm = Path::new("D:/AI_Models");
        let res = Path::new("C:/Users/ai/AppData/Local/LIVA");
        assert_eq!(
            target_path(&f("q.gguf", 1, false, true), llm, res),
            llm.join("q.gguf")
        );
        assert_eq!(
            target_path(&f("models/vad.onnx", 1, false, false), llm, res),
            res.join("models/vad.onnx")
        );
    }

    /// Manifest thật trong repo phải parse được và khai đủ nhóm. Nếu ai sửa
    /// `data/models-manifest.json` sai, đây là chỗ đỏ — chứ không phải máy người
    /// dùng lúc bấm "Tải model".
    #[test]
    fn manifest_that_trong_repo_doc_duoc() {
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop();
        p.push(MANIFEST_REL);
        let raw = std::fs::read_to_string(&p).expect("đọc manifest repo");
        let m = parse_manifest(&raw).expect("manifest repo phải hợp lệ");

        assert!(m.files.len() >= 20, "manifest quá ngắn — {}", m.files.len());
        assert!(
            m.groups.values().any(|g| g.required),
            "phải có ít nhất một nhóm bắt buộc"
        );
        for f in &m.files {
            assert!(!f.dest.is_empty());
            assert!(f.bytes > 0, "{} thiếu kích thước tham chiếu", f.dest);
            if f.url.is_none() {
                assert!(
                    f.manual.is_some(),
                    "{} không tải tự động được thì phải có hướng dẫn",
                    f.dest
                );
            }
        }
    }

    /// Ràng buộc chịu lực giữa hai file: model router mặc định phải LÀ file mà
    /// trình tải tải về. Lệch là máy mới cài tải xong vẫn không chat được —
    /// đúng kiểu hỏng im lặng mà toàn bộ đợt này sinh ra để diệt.
    #[test]
    fn router_mac_dinh_khop_manifest() {
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop();
        p.push(MANIFEST_REL);
        let m = parse_manifest(&std::fs::read_to_string(&p).unwrap()).unwrap();
        let chat: Vec<&ModelFile> = m.files.iter().filter(|f| f.group == "chat").collect();
        assert_eq!(chat.len(), 1, "nhóm chat phải có đúng một model router");
        assert_eq!(
            chat[0].dest,
            crate::DEFAULT_ROUTER_MODEL,
            "DEFAULT_ROUTER_MODEL và manifest đang trỏ hai file khác nhau"
        );
        assert!(chat[0].llm, "model router phải nằm dưới thư mục LLM");
    }

    #[test]
    fn profile_minimal_la_tap_con_that_su_cua_full() {
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop();
        p.push(MANIFEST_REL);
        let m = parse_manifest(&std::fs::read_to_string(&p).unwrap()).unwrap();
        let it = for_profile(&m, "minimal").len();
        let het = for_profile(&m, "full").len();
        assert!(it > 0 && it < het, "minimal={it} full={het}");
    }

    /// Thiếu file của nhóm BẮT BUỘC phải là `blocking` — đó là tín hiệu UI dùng
    /// để chặn màn hình chính và bắt tải trước.
    #[test]
    fn thieu_nhom_bat_buoc_thi_chan() {
        let raw = r#"{
          "version": 1,
          "groups": {
            "stt": {"name":"Nghe","required":true,"broken":"không nghe được","note":""},
            "wake": {"name":"Wake","required":false,"broken":"","note":""}
          },
          "files": [
            {"group":"stt","profile":"minimal","dest":"models/a.onnx","url":"https://x","bytes":10,
             "sha256":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"},
            {"group":"wake","profile":"minimal","dest":"models/b.onnx","url":"https://x","bytes":20,
             "sha256":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}
          ]
        }"#;
        let m = parse_manifest(raw).unwrap();
        let trong = std::env::temp_dir().join("liva_setup_test_khong_ton_tai");
        let st = status(&m, "minimal", &trong, &trong);
        assert!(st.blocking, "thiếu model nhóm bắt buộc phải chặn");
        assert_eq!(st.missing.len(), 2);
        assert_eq!(st.missing_bytes, 30);
        assert_eq!(
            st.groups.first().map(|g| g.key.as_str()),
            Some("stt"),
            "nhóm bắt buộc chưa xong phải đứng đầu để UI hiện trước"
        );
    }

    /// Băm đúng chuẩn: vector thử của SHA-256 trên chuỗi rỗng và "abc".
    #[test]
    fn bam_sha256_dung_chuan() {
        assert_eq!(
            hex_sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex_sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// **Bất biến chịu lực của cả đợt harden này:** file ĐÚNG KÍCH THƯỚC nhưng
    /// SAI NỘI DUNG phải bị từ chối.
    ///
    /// Đây không phải giả thiết. Ngày 28/07/2026, bốn file trên chính máy dev có
    /// đúng số byte manifest ghi mà hash khác nguồn — trong đó ba file VieNeu
    /// khớp kích thước tới từng byte. Cổng cũ (chỉ so kích thước) cho cả bốn đi qua.
    #[test]
    fn cung_kich_thuoc_khac_noi_dung_van_phai_bi_tu_choi() {
        let that = b"noi dung that";
        let gia = b"noi dung GIA!"; // cùng 13 byte
        assert_eq!(that.len(), gia.len(), "test phải dựng đúng tình huống");

        let mong_doi = hex_sha256(that);
        assert!(
            kiem_hash(&mong_doi, gia).is_err(),
            "cùng kích thước mà khác nội dung PHẢI là lỗi"
        );
        assert!(kiem_hash(&mong_doi, that).is_ok());
    }

    /// Thông báo lỗi phải nói được cả hai phía, nếu không người nhận không có
    /// cách nào biết mình đang bị tấn công hay chỉ tải dở.
    #[test]
    fn loi_hash_noi_ro_mong_doi_va_thuc_te() {
        let e = kiem_hash(&hex_sha256(b"a"), b"b").expect_err("phải lỗi");
        assert!(e.contains(&hex_sha256(b"a")), "thiếu hash mong đợi: {e}");
        assert!(e.contains(&hex_sha256(b"b")), "thiếu hash thực tế: {e}");
    }

    /// Manifest thiếu `sha256` cho entry có `url` ⇒ DỪNG, không tải mù.
    #[test]
    fn manifest_thieu_hash_thi_tu_choi() {
        let raw = r#"{"version":1,
            "groups":{"stt":{"name":"Nghe","required":true,"broken":"","note":""}},
            "files":[{"group":"stt","profile":"minimal","dest":"models/a.onnx",
                      "url":"https://x/a.onnx","bytes":10}]}"#;
        let e = parse_manifest(raw).expect_err("thiếu hash phải bị từ chối");
        assert!(e.contains("sha256"), "{e}");
    }

    /// Hash sai định dạng cũng phải chặn — một chuỗi 63 ký tự hay có ký tự lạ là
    /// dấu hiệu manifest bị sửa tay hỏng, và tin nó thì cổng coi như không có.
    #[test]
    fn manifest_hash_sai_dinh_dang_thi_tu_choi() {
        for xau in [
            "abc",
            "ZZZ6816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015a",
        ] {
            let raw = format!(
                r#"{{"version":1,
                "groups":{{"stt":{{"name":"N","required":true,"broken":"","note":""}}}},
                "files":[{{"group":"stt","profile":"minimal","dest":"models/a.onnx",
                          "url":"https://x/a.onnx","bytes":10,"sha256":"{xau}"}}]}}"#
            );
            let e = parse_manifest(&raw).expect_err("phải từ chối hash sai: {xau}");
            assert!(e.contains("sha256"), "{e}");
        }
    }

    /// Entry KHÔNG có url (tự train/tự export) thì không đòi hash — không có
    /// nguồn để đối chiếu, và đòi hash ở đó chỉ là nghi thức.
    #[test]
    fn entry_khong_co_url_thi_khong_doi_hash() {
        let raw = r#"{"version":1,
            "groups":{"wake":{"name":"W","required":false,"broken":"","note":""}},
            "files":[{"group":"wake","profile":"full","dest":"models/w.onnx",
                      "url":null,"bytes":10,"manual":"tự train"}]}"#;
        parse_manifest(raw).expect("entry thủ công vẫn hợp lệ");
    }

    /// Manifest THẬT phải có hash hợp lệ cho mọi entry tải được — đây là chỗ đỏ
    /// nếu ai thêm một model mới mà quên hash.
    #[test]
    fn manifest_that_co_du_hash() {
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop();
        p.push(MANIFEST_REL);
        let m = parse_manifest(&std::fs::read_to_string(&p).unwrap()).unwrap();
        let co_url = m.files.iter().filter(|f| f.url.is_some()).count();
        assert!(co_url >= 20, "chỉ {co_url} entry tải được?");
        for f in m.files.iter().filter(|f| f.url.is_some()) {
            let h = f.sha256.as_deref().unwrap_or("");
            assert!(la_hex_sha256(h), "{} có sha256 không hợp lệ", f.dest);
        }
    }

    /// URL phải đã ghim revision bất biến. Một URL trỏ `main`/`master` nghĩa là
    /// nội dung có thể đổi dưới chân ta — hash sẽ chặn được, nhưng chặn xong thì
    /// người dùng KHÔNG tải được gì nữa, nên đó vẫn là hỏng.
    #[test]
    fn url_khong_con_tro_nhanh_di_dong() {
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop();
        p.push(MANIFEST_REL);
        let m = parse_manifest(&std::fs::read_to_string(&p).unwrap()).unwrap();
        for f in m.files.iter().filter_map(|f| f.url.as_deref()) {
            assert!(
                !f.contains("/resolve/main/") && !f.contains("/resolve/master/"),
                "URL HuggingFace còn trỏ nhánh di động: {f}"
            );
            assert!(
                !f.contains("/raw/main/") && !f.contains("/raw/master/"),
                "URL GitHub còn trỏ nhánh di động: {f}"
            );
        }
    }

    #[test]
    fn manifest_khai_nhom_khong_ton_tai_thi_bao_loi() {
        let raw = r#"{"version":1,"groups":{},"files":[
            {"group":"ma","profile":"minimal","dest":"x","url":null,"bytes":1}]}"#;
        let e = parse_manifest(raw).expect_err("phải từ chối");
        assert!(e.contains("chưa khai báo"), "{e}");
    }
}
