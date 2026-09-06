//! Lưu skill vào SQLite: danh tính, DAG version, sổ tín hiệu.
//!
//! Ba bảng dựng ở migration 4 (`db.rs`). Xem comment tại đó về vai của từng bảng.

use super::SignalTally;
use crate::db::DatabasePool;
use rusqlite::{OptionalExtension, params};

/// Một skill như DB đang giữ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRecord {
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub dir_path: String,
    pub current_version_id: Option<String>,
    pub updated_at: i64,
}

/// Một mắt trong DAG version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillVersion {
    pub version_id: String,
    pub skill_id: String,
    /// `None` cho version đầu tiên của skill.
    pub parent_id: Option<String>,
    pub body: String,
    pub body_sha: String,
    pub created_at: i64,
}

/// Một dòng tín hiệu chất lượng. **G2 chỉ ghi và đọc lại;** dùng nó làm prior khi
/// xếp hạng là G3.
///
/// Các trường theo đúng taxonomy ở §2 tài liệu 04 để G3 không phải migrate lại.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Signal {
    pub skill_id: String,
    pub version_id: Option<String>,
    /// `tool_call_failed` · `tool_failure_affects_skill` ·
    /// `skill_selection_not_invoked` · `tool_semantic_issue`
    pub kind: String,
    pub actionability: Option<String>,
    pub evidence_status: Option<String>,
    pub failure_signature: Option<String>,
    /// Khoá gộp: hai tín hiệu cùng `merge_key` là **cùng một vấn đề** quan sát
    /// nhiều lần, không phải hai vấn đề.
    pub merge_key: Option<String>,
    pub detail: Option<String>,
}

/// Kho skill trên một `DatabasePool`.
pub struct SkillStore<'a> {
    db: &'a DatabasePool,
}

fn bay_gio() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl<'a> SkillStore<'a> {
    pub fn new(db: &'a DatabasePool) -> Self {
        Self { db }
    }

    /// Ghi (hoặc cập nhật) một skill đã đọc từ đĩa, và **thêm version mới nếu nội
    /// dung đổi**.
    ///
    /// Trả `Ok(Some(version_id))` khi có version mới, `Ok(None)` khi nội dung
    /// không đổi — đó là ca thường gặp nhất (quét lại cùng một cây skill), và nó
    /// **không** được sinh rác trong DAG.
    ///
    /// `parent_id` của version mới là version hiện hành trước đó ⇒ DAG tuyến tính
    /// khi chỉ có một nguồn sửa. Kiểu DAG (nhiều nhánh) là để G4 dùng: một bản vá
    /// do OpenSpace đề xuất có thể mọc từ một version cũ mà không ghi đè nhánh
    /// người dùng đang chạy.
    pub fn upsert(&self, s: &super::LoadedSkill) -> Result<Option<String>, String> {
        let conn = self.db.writer.get().map_err(|e| e.to_string())?;
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let now = bay_gio();
        let dir = s.dir_path.to_string_lossy().to_string();

        let hien_hanh: Option<(Option<String>, Option<String>)> = tx
            .query_row(
                "SELECT current_version_id,
                        (SELECT body_sha FROM skill_versions WHERE version_id = current_version_id)
                 FROM skills WHERE skill_id = ?1",
                params![s.skill_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;

        // Metadata (name/description/dir) luôn được đồng bộ, kể cả khi thân bài
        // không đổi — đổi `description` mà không đổi `body` là ca thật, và nó ảnh
        // hưởng truy hồi.
        tx.execute(
            "INSERT INTO skills (skill_id, name, description, dir_path, current_version_id, updated_at)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5)
             ON CONFLICT(skill_id) DO UPDATE SET
                 name = excluded.name,
                 description = excluded.description,
                 dir_path = excluded.dir_path,
                 updated_at = excluded.updated_at",
            params![s.skill_id, s.name, s.description, dir, now],
        )
        .map_err(|e| e.to_string())?;

        let (parent, sha_cu) = match hien_hanh {
            Some((v, sha)) => (v, sha),
            None => (None, None),
        };
        if sha_cu.as_deref() == Some(s.body_sha.as_str()) {
            tx.commit().map_err(|e| e.to_string())?;
            return Ok(None); // nội dung không đổi
        }

        let version_id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO skill_versions (version_id, skill_id, parent_id, body, body_sha, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![version_id, s.skill_id, parent, s.body, s.body_sha, now],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE skills SET current_version_id = ?1, updated_at = ?2 WHERE skill_id = ?3",
            params![version_id, now, s.skill_id],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(Some(version_id))
    }

    /// Đồng bộ cả một cây skill. Trả về `(số skill, số version mới)`.
    pub fn sync_tree(&self, root: &std::path::Path) -> Result<(usize, usize), String> {
        let ds = super::load_skill_tree(root)?;
        let mut moi = 0usize;
        for s in &ds {
            if self.upsert(s)?.is_some() {
                moi += 1;
            }
        }
        tracing::info!(
            "kho skill: {} skill từ {}, {} version mới",
            ds.len(),
            root.display(),
            moi
        );
        Ok((ds.len(), moi))
    }

    pub fn list(&self) -> Result<Vec<SkillRecord>, String> {
        let conn = self.db.writer.get().map_err(|e| e.to_string())?;
        let mut st = conn
            .prepare(
                "SELECT skill_id, name, description, dir_path, current_version_id, updated_at
                 FROM skills ORDER BY name",
            )
            .map_err(|e| e.to_string())?;
        let ra = st
            .query_map([], |r| {
                Ok(SkillRecord {
                    skill_id: r.get(0)?,
                    name: r.get(1)?,
                    description: r.get(2)?,
                    dir_path: r.get(3)?,
                    current_version_id: r.get(4)?,
                    updated_at: r.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(ra)
    }

    /// Thân bài của bản hiện hành, dùng cho truy hồi.
    pub fn current_body(&self, skill_id: &str) -> Result<Option<String>, String> {
        let conn = self.db.writer.get().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT v.body FROM skills s
             JOIN skill_versions v ON v.version_id = s.current_version_id
             WHERE s.skill_id = ?1",
            params![skill_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())
    }

    /// Chuỗi lịch sử của một skill, từ bản hiện hành lần về gốc theo `parent_id`.
    ///
    /// Có chặn trên `MAX_LICH_SU` bước: `parent_id` là dữ liệu, và một chu trình
    /// (do lỗi hoặc do ai đó sửa DB tay) không được làm treo tiến trình.
    pub fn history(&self, skill_id: &str) -> Result<Vec<SkillVersion>, String> {
        const MAX_LICH_SU: usize = 1000;
        let conn = self.db.writer.get().map_err(|e| e.to_string())?;
        let mut hien: Option<String> = conn
            .query_row(
                "SELECT current_version_id FROM skills WHERE skill_id = ?1",
                params![skill_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .flatten();

        let mut ra = Vec::new();
        let mut da_gap = std::collections::HashSet::new();
        while let Some(vid) = hien {
            if !da_gap.insert(vid.clone()) {
                tracing::warn!("DAG version của skill '{skill_id}' có chu trình tại {vid}; dừng");
                break;
            }
            if ra.len() >= MAX_LICH_SU {
                tracing::warn!("lịch sử skill '{skill_id}' vượt {MAX_LICH_SU} bản; CẮT");
                break;
            }
            let v: Option<SkillVersion> = conn
                .query_row(
                    "SELECT version_id, skill_id, parent_id, body, body_sha, created_at
                     FROM skill_versions WHERE version_id = ?1",
                    params![vid],
                    |r| {
                        Ok(SkillVersion {
                            version_id: r.get(0)?,
                            skill_id: r.get(1)?,
                            parent_id: r.get(2)?,
                            body: r.get(3)?,
                            body_sha: r.get(4)?,
                            created_at: r.get(5)?,
                        })
                    },
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match v {
                Some(v) => {
                    hien = v.parent_id.clone();
                    ra.push(v);
                }
                None => break,
            }
        }
        Ok(ra)
    }

    /// Ghi một tín hiệu chất lượng. **G2 chỉ ghi;** đọc nó vào xếp hạng là G3.
    pub fn record_signal(&self, s: &Signal) -> Result<i64, String> {
        let conn = self.db.writer.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO skill_signals
                 (skill_id, version_id, kind, actionability, evidence_status,
                  failure_signature, merge_key, detail, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                s.skill_id,
                s.version_id,
                s.kind,
                s.actionability,
                s.evidence_status,
                s.failure_signature,
                s.merge_key,
                s.detail,
                bay_gio()
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }

    /// Đếm **vấn đề phân biệt** cho nhiều skill một lượt — đầu vào của prior G3.
    ///
    /// Khác [`Self::signal_counts`] ở đúng một điểm, và điểm đó là cả lý do hàm này
    /// tồn tại: `signal_counts` dùng `COUNT(*)` thô, tức đếm **lần quan sát**. Với
    /// prior xếp hạng thì đó là con số sai — `merge_key` được định nghĩa là "cùng
    /// một vấn đề quan sát nhiều lần", nên một sự cố lặp 20 lần sẽ đọc thành 20 lỗi
    /// và dìm chết một skill vốn chỉ có một vấn đề.
    ///
    /// `COUNT(DISTINCT merge_key)` một mình thì lại bỏ sót: SQLite **không đếm
    /// NULL** trong `DISTINCT`, mà `merge_key` là cột cho phép NULL. Tín hiệu không
    /// có khoá gộp là tín hiệu chưa ai gom — mỗi dòng là một vấn đề riêng cho tới
    /// khi có người chứng minh ngược lại. Nên tổng = (số khoá phân biệt) + (số dòng
    /// NULL), tính bằng hai `SUM(CASE ...)` trên cùng một lượt quét.
    ///
    /// Trả về một [`SignalTally`] cho **mỗi** `skill_id` được hỏi, kể cả skill không
    /// có tín hiệu nào (tally rỗng) — người gọi khỏi phải phân biệt "không có tín
    /// hiệu" với "không có trong map".
    pub fn signal_tallies(
        &self,
        skill_ids: &[String],
    ) -> Result<std::collections::HashMap<String, SignalTally>, String> {
        let mut ra: std::collections::HashMap<String, SignalTally> = skill_ids
            .iter()
            .map(|id| (id.clone(), SignalTally::default()))
            .collect();
        if skill_ids.is_empty() {
            return Ok(ra);
        }
        let conn = self.db.writer.get().map_err(|e| e.to_string())?;

        // Một placeholder cho mỗi id. Không nội suy chuỗi vào SQL — `skill_id` đến
        // từ payload lệnh trên WS 8002 (không xác thực), nên nó là dữ liệu người
        // ngoài kiểm soát.
        let cho = vec!["?"; skill_ids.len()].join(",");
        let sql = format!(
            "SELECT skill_id, kind, evidence_status,
                    COUNT(DISTINCT merge_key)
                      + SUM(CASE WHEN merge_key IS NULL THEN 1 ELSE 0 END) AS n
             FROM skill_signals
             WHERE skill_id IN ({cho})
             GROUP BY skill_id, kind, evidence_status
             ORDER BY skill_id, kind, evidence_status"
        );
        let mut st = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let hang = st
            .query_map(rusqlite::params_from_iter(skill_ids.iter()), |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        for (skill_id, kind, ev, n) in hang {
            if let Some(t) = ra.get_mut(&skill_id) {
                t.theo_loai.push((kind, ev, n));
            }
        }
        Ok(ra)
    }

    /// Đếm tín hiệu theo `kind` cho một skill — **số lần quan sát**, không phải số
    /// vấn đề.
    ///
    /// Dùng cho việc báo cáo/chẩn đoán ("chuyện này xảy ra bao nhiêu lần rồi?").
    /// Cho prior xếp hạng thì dùng [`Self::signal_tallies`] — xem lý do ở đó.
    pub fn signal_counts(&self, skill_id: &str) -> Result<Vec<(String, i64)>, String> {
        let conn = self.db.writer.get().map_err(|e| e.to_string())?;
        let mut st = conn
            .prepare(
                "SELECT kind, COUNT(*) FROM skill_signals
                 WHERE skill_id = ?1 GROUP BY kind ORDER BY kind",
            )
            .map_err(|e| e.to_string())?;
        let ra = st
            .query_map(params![skill_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(ra)
    }
}

/// In-memory fast cache and index of loaded ClawHub skill packages.
#[derive(Debug, Default, Clone)]
pub struct SkillPackageStore {
    packages: std::collections::HashMap<String, super::manifest::LoadedSkillPackage>,
}

impl SkillPackageStore {
    pub fn new() -> Self {
        Self {
            packages: std::collections::HashMap::new(),
        }
    }

    /// Upsert a skill package. Returns true if added or hash changed.
    pub fn upsert(&mut self, pkg: super::manifest::LoadedSkillPackage) -> bool {
        let name = pkg.manifest.name.clone();
        match self.packages.get(&name) {
            Some(existing) if existing.content_hash == pkg.content_hash => false,
            _ => {
                self.packages.insert(name, pkg);
                true
            }
        }
    }

    pub fn remove(&mut self, skill_name: &str) -> Option<super::manifest::LoadedSkillPackage> {
        self.packages.remove(skill_name)
    }

    pub fn get(&self, skill_name: &str) -> Option<&super::manifest::LoadedSkillPackage> {
        self.packages.get(skill_name)
    }

    pub fn list(&self) -> Vec<super::manifest::LoadedSkillPackage> {
        self.packages.values().cloned().collect()
    }

    pub fn count(&self) -> usize {
        self.packages.len()
    }

    pub fn find_by_trigger(&self, query: &str) -> Vec<super::manifest::LoadedSkillPackage> {
        let query_lower = query.to_lowercase();
        self.packages
            .values()
            .filter(|pkg| {
                pkg.manifest.triggers.iter().any(|trig| match trig {
                    super::manifest::SkillTrigger::Intent(s)
                    | super::manifest::SkillTrigger::Regex(s)
                    | super::manifest::SkillTrigger::Event(s)
                    | super::manifest::SkillTrigger::Cron(s) => {
                        s.to_lowercase().contains(&query_lower) || query_lower.contains(&s.to_lowercase())
                    }
                    super::manifest::SkillTrigger::Keyword(keywords) => {
                        keywords.iter().any(|k| query_lower.contains(&k.to_lowercase()))
                    }
                }) || pkg.manifest.name.to_lowercase().contains(&query_lower)
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::LoadedSkill;
    use std::path::PathBuf;

    fn db() -> DatabasePool {
        DatabasePool::new_in_memory().expect("DB in-memory")
    }

    fn skill(id: &str, name: &str, body: &str) -> LoadedSkill {
        LoadedSkill {
            skill_id: id.to_string(),
            name: name.to_string(),
            description: format!("mô tả của {name}"),
            body: body.to_string(),
            // ĐÚNG hàm sha của loader, không phải bản sao trong test.
            body_sha: crate::skills::loader::sha_hex(body),
            dir_path: PathBuf::from(format!("/skills/{name}")),
        }
    }

    #[test]
    fn upsert_lan_dau_tao_skill_va_version_goc() {
        let d = db();
        let st = SkillStore::new(&d);
        let v = st.upsert(&skill("id-1", "aaa", "thân 1")).expect("upsert");
        assert!(v.is_some(), "lần đầu phải tạo version");

        let ds = st.list().unwrap();
        assert_eq!(ds.len(), 1);
        assert_eq!(ds[0].name, "aaa");
        assert_eq!(ds[0].current_version_id, v);

        let h = st.history("id-1").unwrap();
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].parent_id, None, "version gốc không có cha");
    }

    /// Ca thường gặp nhất: quét lại cùng một cây skill. Không được sinh rác.
    #[test]
    fn upsert_lai_cung_noi_dung_khong_tao_version_moi() {
        let d = db();
        let st = SkillStore::new(&d);
        let s = skill("id-1", "aaa", "thân 1");
        st.upsert(&s).unwrap();
        assert_eq!(st.upsert(&s).unwrap(), None, "nội dung không đổi ⇒ None");
        assert_eq!(st.history("id-1").unwrap().len(), 1, "DAG không được phình");
    }

    #[test]
    fn doi_than_bai_tao_version_moi_noi_ve_cha() {
        let d = db();
        let st = SkillStore::new(&d);
        let v1 = st.upsert(&skill("id-1", "aaa", "thân 1")).unwrap().unwrap();
        let v2 = st.upsert(&skill("id-1", "aaa", "thân 2")).unwrap().unwrap();
        assert_ne!(v1, v2);

        let h = st.history("id-1").unwrap();
        assert_eq!(h.len(), 2, "lịch sử phải có 2 bản");
        assert_eq!(h[0].version_id, v2, "bản mới nhất đứng đầu");
        assert_eq!(
            h[0].parent_id.as_deref(),
            Some(v1.as_str()),
            "phải trỏ về cha"
        );
        assert_eq!(h[1].parent_id, None);
        assert_eq!(st.current_body("id-1").unwrap().as_deref(), Some("thân 2"));
    }

    /// Đổi `description` mà không đổi `body` là ca THẬT, và nó ảnh hưởng truy hồi
    /// — nên metadata phải được đồng bộ dù DAG không thêm mắt nào.
    #[test]
    fn doi_mo_ta_khong_doi_than_van_duoc_dong_bo() {
        let d = db();
        let st = SkillStore::new(&d);
        st.upsert(&skill("id-1", "aaa", "thân")).unwrap();
        let mut s = skill("id-1", "aaa", "thân");
        s.description = "mô tả HOÀN TOÀN mới".to_string();
        assert_eq!(
            st.upsert(&s).unwrap(),
            None,
            "thân không đổi ⇒ không version mới"
        );
        assert_eq!(st.list().unwrap()[0].description, "mô tả HOÀN TOÀN mới");
    }

    /// Danh tính là `skill_id`: đổi `name` không tạo skill thứ hai.
    #[test]
    fn doi_name_khong_tao_skill_thu_hai() {
        let d = db();
        let st = SkillStore::new(&d);
        st.upsert(&skill("id-1", "ten-cu", "thân")).unwrap();
        st.upsert(&skill("id-1", "ten-moi", "thân")).unwrap();
        let ds = st.list().unwrap();
        assert_eq!(ds.len(), 1, "vẫn phải là MỘT skill");
        assert_eq!(ds[0].name, "ten-moi");
    }

    #[test]
    fn ghi_va_dem_duoc_tin_hieu() {
        let d = db();
        let st = SkillStore::new(&d);
        let v = st.upsert(&skill("id-1", "aaa", "thân")).unwrap();
        for k in [
            "tool_call_failed",
            "tool_call_failed",
            "tool_semantic_issue",
        ] {
            st.record_signal(&Signal {
                skill_id: "id-1".to_string(),
                version_id: v.clone(),
                kind: k.to_string(),
                merge_key: Some("cung-mot-van-de".to_string()),
                ..Default::default()
            })
            .expect("ghi tín hiệu");
        }
        let dem = st.signal_counts("id-1").unwrap();
        assert_eq!(
            dem,
            vec![
                ("tool_call_failed".to_string(), 2),
                ("tool_semantic_issue".to_string(), 1)
            ]
        );
    }

    /// Đây là **cả lý do** `signal_tallies` tồn tại tách khỏi `signal_counts`: hai
    /// lần quan sát CÙNG `merge_key` là một vấn đề, không phải hai. Nếu prior đếm
    /// theo dòng thì một sự cố lặp lại đủ dìm chết một skill.
    #[test]
    fn tally_dem_van_de_phan_biet_chu_khong_dem_lan_quan_sat() {
        let d = db();
        let st = SkillStore::new(&d);
        st.upsert(&skill("id-1", "aaa", "thân")).unwrap();
        // Một vấn đề, quan sát 5 lần.
        for _ in 0..5 {
            st.record_signal(&Signal {
                skill_id: "id-1".to_string(),
                kind: "tool_call_failed".to_string(),
                merge_key: Some("van-de-A".to_string()),
                ..Default::default()
            })
            .unwrap();
        }
        // Vấn đề thứ hai, quan sát 1 lần.
        st.record_signal(&Signal {
            skill_id: "id-1".to_string(),
            kind: "tool_call_failed".to_string(),
            merge_key: Some("van-de-B".to_string()),
            ..Default::default()
        })
        .unwrap();

        let quan_sat = st.signal_counts("id-1").unwrap();
        assert_eq!(quan_sat, vec![("tool_call_failed".to_string(), 6)], "6 LẦN");

        let t = &st.signal_tallies(&["id-1".to_string()]).unwrap()["id-1"];
        assert_eq!(
            t.theo_loai,
            vec![("tool_call_failed".to_string(), None, 2)],
            "nhưng chỉ 2 VẤN ĐỀ"
        );
    }

    /// SQLite **không đếm NULL** trong `COUNT(DISTINCT ...)`. Tín hiệu chưa có
    /// `merge_key` là tín hiệu chưa ai gom ⇒ mỗi dòng là một vấn đề riêng. Không có
    /// nhánh `SUM(CASE ...)` thì toàn bộ nhóm này biến mất khỏi prior — im lặng.
    #[test]
    fn tally_khong_bo_sot_tin_hieu_thieu_merge_key() {
        let d = db();
        let st = SkillStore::new(&d);
        st.upsert(&skill("id-1", "aaa", "thân")).unwrap();
        for _ in 0..3 {
            st.record_signal(&Signal {
                skill_id: "id-1".to_string(),
                kind: "tool_semantic_issue".to_string(),
                merge_key: None,
                ..Default::default()
            })
            .unwrap();
        }
        let t = &st.signal_tallies(&["id-1".to_string()]).unwrap()["id-1"];
        assert_eq!(
            t.theo_loai,
            vec![("tool_semantic_issue".to_string(), None, 3)],
            "3 dòng NULL = 3 vấn đề, không phải 0"
        );
        assert!(t.hinh_phat() > 0.0, "và phải thật sự sinh hình phạt");
    }

    /// Trộn: cùng `kind` nhưng khác `evidence_status` phải tách nhóm, vì hai mức
    /// bằng chứng có trọng số khác nhau.
    #[test]
    fn tally_tach_nhom_theo_muc_bang_chung() {
        let d = db();
        let st = SkillStore::new(&d);
        st.upsert(&skill("id-1", "aaa", "thân")).unwrap();
        for (ev, mk) in [
            (Some("confirmed"), "A"),
            (Some("refuted"), "B"),
            (None, "C"),
        ] {
            st.record_signal(&Signal {
                skill_id: "id-1".to_string(),
                kind: "tool_failure_affects_skill".to_string(),
                evidence_status: ev.map(str::to_string),
                merge_key: Some(mk.to_string()),
                ..Default::default()
            })
            .unwrap();
        }
        let t = &st.signal_tallies(&["id-1".to_string()]).unwrap()["id-1"];
        assert_eq!(
            t.theo_loai.len(),
            3,
            "ba mức bằng chứng ⇒ ba nhóm: {:?}",
            t.theo_loai
        );
        // confirmed(1,0) + refuted(0,0) + chưa rõ(0,5) = 1,5
        assert!(
            (t.tong_trong_so() - 1.5).abs() < 1e-6,
            "{}",
            t.tong_trong_so()
        );
    }

    /// Hỏi nhiều skill một lượt: skill không có tín hiệu vẫn phải CÓ mặt trong map
    /// với tally rỗng, để người gọi khỏi phân biệt "sạch" với "thiếu khoá".
    #[test]
    fn tally_tra_du_khoa_ke_ca_skill_sach() {
        let d = db();
        let st = SkillStore::new(&d);
        st.upsert(&skill("id-1", "aaa", "thân")).unwrap();
        st.upsert(&skill("id-2", "bbb", "thân")).unwrap();
        st.record_signal(&Signal {
            skill_id: "id-1".to_string(),
            kind: "tool_call_failed".to_string(),
            ..Default::default()
        })
        .unwrap();

        let m = st
            .signal_tallies(&[
                "id-1".to_string(),
                "id-2".to_string(),
                "khong-co".to_string(),
            ])
            .unwrap();
        assert_eq!(m.len(), 3, "đủ ba khoá kể cả skill_id không tồn tại");
        assert!(m["id-1"].hinh_phat() > 0.0);
        assert_eq!(m["id-2"].hinh_phat(), 0.0, "skill sạch ⇒ phạt 0");
        assert_eq!(m["khong-co"].hinh_phat(), 0.0);
        assert!(st.signal_tallies(&[]).unwrap().is_empty(), "danh sách rỗng");
    }

    #[test]
    fn skill_khong_ton_tai_thi_tra_rong_chu_khong_loi() {
        let d = db();
        let st = SkillStore::new(&d);
        assert!(st.history("khong-co").unwrap().is_empty());
        assert!(st.current_body("khong-co").unwrap().is_none());
        assert!(st.signal_counts("khong-co").unwrap().is_empty());
    }
}
