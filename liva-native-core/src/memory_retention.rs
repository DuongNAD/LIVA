//! Retention opt-in cho conversation memory.
//!
//! Mặc định **tắt**: không có chính sách thì runtime không tự xóa dữ liệu.
//! Khi `LIVA_MEMORY_RETENTION_DAYS` là số dương, worker xử lý batch nhỏ ngoài
//! async executor và dựa trên dữ liệu còn lại làm checkpoint idempotent.

const DEFAULT_INTERVAL_SECS: u64 = 3_600;
const MIN_INTERVAL_SECS: u64 = 60;
const DEFAULT_BATCH_SIZE: usize = 10;
const MAX_BATCH_SIZE: usize = 25;
const MAX_RETENTION_DAYS: u64 = 36_500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionPolicy {
    pub max_age_days: u64,
    pub interval: std::time::Duration,
    pub batch_size: usize,
}

impl RetentionPolicy {
    pub fn from_env() -> Option<Self> {
        match Self::from_values(
            std::env::var("LIVA_MEMORY_RETENTION_DAYS").ok().as_deref(),
            std::env::var("LIVA_MEMORY_RETENTION_INTERVAL_SECS")
                .ok()
                .as_deref(),
            std::env::var("LIVA_MEMORY_RETENTION_BATCH_SIZE")
                .ok()
                .as_deref(),
        ) {
            Ok(policy) => policy,
            Err(error) => {
                tracing::warn!(%error, "memory retention disabled because configuration is invalid");
                None
            }
        }
    }

    fn from_values(
        days: Option<&str>,
        interval_secs: Option<&str>,
        batch_size: Option<&str>,
    ) -> Result<Option<Self>, String> {
        let Some(days) = days.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(None);
        };
        let days = days
            .parse::<u64>()
            .map_err(|_| "LIVA_MEMORY_RETENTION_DAYS must be an integer".to_string())?;
        if days == 0 {
            return Ok(None);
        }
        if days > MAX_RETENTION_DAYS {
            return Err(format!(
                "LIVA_MEMORY_RETENTION_DAYS must not exceed {MAX_RETENTION_DAYS}"
            ));
        }
        let interval_secs = parse_bounded(
            interval_secs,
            "LIVA_MEMORY_RETENTION_INTERVAL_SECS",
            DEFAULT_INTERVAL_SECS,
            MIN_INTERVAL_SECS,
            u64::MAX,
        )?;
        let batch_size = parse_bounded(
            batch_size,
            "LIVA_MEMORY_RETENTION_BATCH_SIZE",
            DEFAULT_BATCH_SIZE as u64,
            1,
            MAX_BATCH_SIZE as u64,
        )? as usize;
        Ok(Some(Self {
            max_age_days: days,
            interval: std::time::Duration::from_secs(interval_secs),
            batch_size,
        }))
    }
}

fn parse_bounded(
    raw: Option<&str>,
    name: &str,
    default: u64,
    min: u64,
    max: u64,
) -> Result<u64, String> {
    let value = match raw.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => value
            .parse::<u64>()
            .map_err(|_| format!("{name} must be an integer"))?,
        None => default,
    };
    if !(min..=max).contains(&value) {
        return Err(format!("{name} must be from {min} through {max}"));
    }
    Ok(value)
}

pub fn spawn_retention_sweeper(
    db: crate::db::DatabasePool,
    policy: RetentionPolicy,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run_retention_sweeper(db, policy))
}

pub async fn run_retention_sweeper(db: crate::db::DatabasePool, policy: RetentionPolicy) {
    let mut interval = tokio::time::interval(policy.interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let db = db.clone();
        let policy = policy.clone();
        match tokio::task::spawn_blocking(move || sweep_once(&db, &policy)).await {
            Ok(Ok(report)) if !report.deletions.is_empty() => tracing::info!(
                deleted = report.deletions.len(),
                has_more = report.has_more,
                cutoff_ms = report.cutoff_ms,
                "memory retention batch completed"
            ),
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                tracing::warn!(%error, "memory retention batch failed; next interval will retry")
            }
            Err(error) => tracing::warn!(%error, "memory retention worker panicked"),
        }
    }
}

fn sweep_once(
    db: &crate::db::DatabasePool,
    policy: &RetentionPolicy,
) -> Result<crate::db::RetentionSweepReport, String> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("system clock is before UNIX epoch: {error}"))?
        .as_millis() as i64;
    let age_ms = i64::try_from(policy.max_age_days)
        .ok()
        .and_then(|days| days.checked_mul(86_400_000))
        .ok_or_else(|| "retention age overflows milliseconds".to_string())?;
    let cutoff_ms = now_ms
        .checked_sub(age_ms)
        .ok_or_else(|| "retention cutoff is before supported time range".to_string())?;
    let conn = db
        .writer
        .get()
        .map_err(|error| format!("cannot acquire DB writer: {error}"))?;
    crate::db::sweep_conversation_retention(&conn, "local", cutoff_ms, policy.batch_size, false)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_mac_dinh_tat_va_chan_cau_hinh_nguy_hiem() {
        assert_eq!(
            RetentionPolicy::from_values(None, None, None).unwrap(),
            None
        );
        assert_eq!(
            RetentionPolicy::from_values(Some("0"), None, None).unwrap(),
            None
        );
        let policy = RetentionPolicy::from_values(Some("30"), None, None)
            .unwrap()
            .unwrap();
        assert_eq!(policy.max_age_days, 30);
        assert_eq!(policy.interval.as_secs(), DEFAULT_INTERVAL_SECS);
        assert_eq!(policy.batch_size, DEFAULT_BATCH_SIZE);
        assert!(RetentionPolicy::from_values(Some("1"), Some("59"), None).is_err());
        assert!(RetentionPolicy::from_values(Some("1"), None, Some("26")).is_err());
        assert!(RetentionPolicy::from_values(Some("36501"), None, None).is_err());
    }
}
