//! Coarse, opt-in location resolution for tools that otherwise need to ask the user.
//!
//! The external service sees the public IP as part of the HTTPS request, but LIVA never
//! requests the IP field, never stores it, never logs it, and never sends it to the LLM.

use serde::Deserialize;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(4);
const IP_LOCATION_URL: &str = "https://ipwho.is/?fields=success,message,city,region,country";

#[derive(Debug, Clone)]
struct CachedLocation {
    value: String,
    stored_at: Instant,
}

#[derive(Debug, Deserialize)]
struct IpLocationResponse {
    success: bool,
    message: Option<String>,
    city: Option<String>,
    region: Option<String>,
    country: Option<String>,
}

static LOCATION_CACHE: OnceLock<Mutex<Option<CachedLocation>>> = OnceLock::new();

fn non_empty(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Applies the privacy-preserving precedence contract without touching the network.
pub(crate) fn choose_known_location(
    explicit: Option<&str>,
    profile: Option<&str>,
    geolocation_enabled: bool,
    cached: Option<&str>,
) -> Result<Option<String>, String> {
    if let Some(location) = non_empty(explicit) {
        return Ok(Some(location));
    }
    if let Some(location) = non_empty(profile) {
        return Ok(Some(location));
    }
    if !geolocation_enabled {
        return Err(
            "Chưa có địa điểm đã lưu và định vị IP đang tắt; hãy cho biết thành phố hoặc bật định vị trong Cài đặt."
                .to_string(),
        );
    }
    Ok(non_empty(cached))
}

fn saved_profile_location() -> Option<String> {
    let path = crate::data_dir().join("user_profile.json");
    let raw = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    non_empty(value.get("location").and_then(serde_json::Value::as_str))
}

fn geolocation_enabled() -> bool {
    if let Ok(raw) = std::env::var("LIVA_GEOLOCATION_ENABLED") {
        return matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        );
    }

    std::fs::read_to_string(crate::config_file_path())
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|config| config.get("system")?.get("geolocationEnabled")?.as_bool())
        .unwrap_or(false)
}

fn cached_location() -> Option<String> {
    let cache = LOCATION_CACHE.get_or_init(|| Mutex::new(None));
    let guard = cache.lock().ok()?;
    let cached = guard.as_ref()?;
    (cached.stored_at.elapsed() < CACHE_TTL).then(|| cached.value.clone())
}

fn store_cached_location(location: String) {
    let cache = LOCATION_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = cache.lock() {
        *guard = Some(CachedLocation {
            value: location,
            stored_at: Instant::now(),
        });
    }
}

fn clean_component(value: Option<String>) -> Option<String> {
    let value: String = value?
        .chars()
        .filter(|character| !character.is_control())
        .take(80)
        .collect();
    non_empty(Some(&value))
}

fn coarse_location(response: IpLocationResponse) -> Result<String, String> {
    if !response.success {
        return Err(response
            .message
            .unwrap_or_else(|| "dịch vụ định vị từ chối yêu cầu".to_string()));
    }

    let mut parts = Vec::with_capacity(3);
    for part in [response.city, response.region, response.country]
        .into_iter()
        .filter_map(clean_component)
    {
        if !parts.iter().any(|existing| existing == &part) {
            parts.push(part);
        }
    }
    if parts.is_empty() {
        return Err("dịch vụ định vị không trả về thành phố hoặc quốc gia".to_string());
    }
    Ok(parts.join(", "))
}

async fn lookup_coarse_location() -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(LOOKUP_TIMEOUT)
        .build()
        .map_err(|error| format!("không dựng được HTTP client: {error}"))?;
    let response = client
        .get(IP_LOCATION_URL)
        .timeout(LOOKUP_TIMEOUT)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| format!("không kết nối được dịch vụ định vị: {error}"))?
        .json::<IpLocationResponse>()
        .await
        .map_err(|error| format!("phản hồi định vị không hợp lệ: {error}"))?;
    coarse_location(response)
}

pub(crate) async fn resolve_location(explicit: Option<&str>) -> Result<String, String> {
    let profile = saved_profile_location();
    let cached = cached_location();
    if let Some(location) = choose_known_location(
        explicit,
        profile.as_deref(),
        geolocation_enabled(),
        cached.as_deref(),
    )? {
        return Ok(location);
    }

    let location = lookup_coarse_location()
        .await
        .map_err(|error| format!("Không xác định được vị trí gần đúng từ IP: {error}"))?;
    store_cached_location(location.clone());
    Ok(location)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chi_giu_thanh_pho_vung_quoc_gia_va_khong_co_ip() {
        let location = coarse_location(IpLocationResponse {
            success: true,
            message: None,
            city: Some("Hanoi".to_string()),
            region: Some("Hanoi".to_string()),
            country: Some("Vietnam".to_string()),
        })
        .unwrap();

        assert_eq!(location, "Hanoi, Vietnam");
    }

    #[tokio::test]
    #[ignore = "chạm Internet — chạy tay, không để CI phụ thuộc dịch vụ định vị"]
    async fn ip_lookup_that_chi_tra_ve_vi_tri_tho() {
        let location = lookup_coarse_location().await.unwrap();
        assert!(!location.trim().is_empty());
        assert!(location.split(',').count() <= 3);
        println!("coarse location: {location}");
    }
}
