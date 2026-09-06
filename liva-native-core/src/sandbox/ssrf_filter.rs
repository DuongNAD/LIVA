//! SSRF Network Filter & Domain Boundary Engine
//!
//! Provides defense-in-depth outbound network filtering:
//! - Blocks cloud metadata endpoints (AWS, GCP, Azure, Alibaba, OpenStack).
//! - Blocks RFC 1918 private subnets (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16).
//! - Blocks Carrier-Grade NAT (RFC 6598: 100.64.0.0/10).
//! - Blocks loopback (127.0.0.0/8, ::1) and unspecified addresses (0.0.0.0, ::).
//! - Blocks IPv6 link-local (fe80::/10), unique-local (fc00::/7), and IPv4-mapped IPv6 (::ffff:x.x.x.x).
//! - Defends against URL userinfo evasion, bracketed IPv6 parsing confusion, and forbidden URI schemes.

use std::net::IpAddr;
use tracing::warn;

use crate::sandbox::policy::{SandboxPolicy, SandboxViolation};

/// SSRF protection and domain allowlist filter.
#[derive(Debug, Default, Clone)]
pub struct SsrfFilter;

impl SsrfFilter {
    /// Creates a new `SsrfFilter`.
    pub fn new() -> Self {
        Self
    }

    /// Checks if an IP address is permitted for outbound connection.
    /// Rejects loopback, private RFC 1918, link-local, cloud metadata, CGNAT, and reserved IPv6 ranges.
    pub fn is_ip_allowed(&self, ip: IpAddr) -> bool {
        if ip.is_loopback() || ip.is_unspecified() {
            return false;
        }

        match ip {
            IpAddr::V4(ipv4) => {
                if ipv4.is_private()
                    || ipv4.is_link_local()
                    || ipv4.is_broadcast()
                    || ipv4.is_documentation()
                {
                    return false;
                }

                let octets = ipv4.octets();

                // 0.0.0.0/8 (current network) and 127.0.0.0/8 (loopback)
                if octets[0] == 0 || octets[0] == 127 {
                    return false;
                }

                // Explicit cloud metadata blocks (AWS, Azure, GCP, Alibaba)
                if octets == [169, 254, 169, 254]
                    || octets == [169, 254, 169, 250]
                    || octets == [100, 100, 100, 200]
                {
                    return false;
                }

                // Carrier-Grade NAT (RFC 6598: 100.64.0.0/10)
                if octets[0] == 100 && (octets[1] & 0xc0) == 64 {
                    return false;
                }

                // Benchmarking (RFC 2544: 198.18.0.0/15)
                if octets[0] == 198 && (octets[1] & 0xfe) == 18 {
                    return false;
                }

                // Class E Reserved (240.0.0.0/4)
                if (octets[0] & 0xf0) == 240 {
                    return false;
                }

                true
            }
            IpAddr::V6(ipv6) => {
                if ipv6.is_loopback() || ipv6.is_unspecified() {
                    return false;
                }

                let segments = ipv6.segments();

                // Handle IPv4-mapped IPv6 addresses (::ffff:x.x.x.x)
                if segments[0] == 0
                    && segments[1] == 0
                    && segments[2] == 0
                    && segments[3] == 0
                    && segments[4] == 0
                    && segments[5] == 0xffff
                {
                    let v4 = std::net::Ipv4Addr::new(
                        (segments[6] >> 8) as u8,
                        (segments[6] & 0xff) as u8,
                        (segments[7] >> 8) as u8,
                        (segments[7] & 0xff) as u8,
                    );
                    return self.is_ip_allowed(IpAddr::V4(v4));
                }

                // Handle IPv4-compatible IPv6 addresses (::x.x.x.x)
                if segments[0] == 0
                    && segments[1] == 0
                    && segments[2] == 0
                    && segments[3] == 0
                    && segments[4] == 0
                    && segments[5] == 0
                    && (segments[6] != 0 || segments[7] != 1)
                {
                    let v4 = std::net::Ipv4Addr::new(
                        (segments[6] >> 8) as u8,
                        (segments[6] & 0xff) as u8,
                        (segments[7] >> 8) as u8,
                        (segments[7] & 0xff) as u8,
                    );
                    return self.is_ip_allowed(IpAddr::V4(v4));
                }

                // fe80::/10 link-local
                if (segments[0] & 0xffc0) == 0xfe80 {
                    return false;
                }

                // fc00::/7 unique local (fc00:: - fdff::)
                if (segments[0] & 0xfe00) == 0xfc00 {
                    return false;
                }

                // 2001:db8::/32 documentation
                if segments[0] == 0x2001 && segments[1] == 0x0db8 {
                    return false;
                }

                true
            }
        }
    }

    /// Validates a URL against default SSRF rules.
    pub fn validate_url(&self, raw_url: &str) -> Result<(), SandboxViolation> {
        let policy = SandboxPolicy::default();
        Self::validate_url_with_policy(raw_url, &policy)
    }

    /// Validates a raw destination URL against policy SSRF rules, IP blocks, and domain allowlists.
    pub fn validate_url_with_policy(
        raw_url: &str,
        policy: &SandboxPolicy,
    ) -> Result<(), SandboxViolation> {
        let trimmed = raw_url.trim();
        let lower = trimmed.to_lowercase();

        // 1. Protocol Scheme Verification (only http and https allowed for outbound network)
        if lower.starts_with("file:")
            || lower.starts_with("javascript:")
            || lower.starts_with("data:")
            || lower.starts_with("gopher:")
            || lower.starts_with("ftp:")
            || lower.starts_with("dict:")
            || lower.starts_with("ldap:")
            || lower.starts_with("tftp:")
        {
            warn!("Disallowed URI scheme prevented: {}", raw_url);
            return Err(SandboxViolation::BlockedDomain(raw_url.to_string()));
        }

        if !lower.starts_with("http://") && !lower.starts_with("https://") {
            warn!("Unsupported URL scheme in: {}", raw_url);
            return Err(SandboxViolation::BlockedDomain(raw_url.to_string()));
        }

        // 2. Direct Cloud Instance Metadata & Loopback string patterns
        let explicit_ssrf = [
            "169.254.169.254",
            "169.254.169.250",
            "100.100.100.200",
            "metadata.google.internal",
            "metadata.google",
            "localhost",
            "127.0.0.1",
            "0.0.0.0",
            "[::1]",
            "::1",
        ];
        for target in &explicit_ssrf {
            if lower.contains(target) {
                warn!("SSRF or metadata access prevented for URL: {}", raw_url);
                return Err(SandboxViolation::SsrfAttempt(raw_url.to_string()));
            }
        }

        // 3. Userinfo inspection and host extraction
        if let Some(pos) = lower.find("://") {
            let rest = &lower[pos + 3..];
            let authority_end = rest
                .find(|c| c == '/' || c == '?' || c == '#')
                .unwrap_or(rest.len());
            let authority = &rest[..authority_end];

            // If userinfo is present (e.g. user@host), inspect userinfo for evasion payloads
            if let Some(at_pos) = authority.rfind('@') {
                let userinfo = &authority[..at_pos];
                for target in &explicit_ssrf {
                    if userinfo.contains(target) {
                        warn!("SSRF attempt in URL userinfo: {}", raw_url);
                        return Err(SandboxViolation::SsrfAttempt(raw_url.to_string()));
                    }
                }
            }
        }

        // 4. Host-specific inspection
        if let Some(host) = Self::extract_host(&lower) {
            if host.ends_with(".local")
                || host.ends_with(".internal")
                || host.ends_with(".corp")
                || host.ends_with(".localhost")
                || host.ends_with(".lan")
                || host.ends_with(".home.arpa")
            {
                warn!("Internal domain access prevented: {}", host);
                return Err(SandboxViolation::SsrfAttempt(raw_url.to_string()));
            }

            if host.starts_with("10.")
                || host.starts_with("192.168.")
                || host.starts_with("169.254.")
                || host.starts_with("127.")
                || host.starts_with("0.")
            {
                warn!("Private/Loopback IPv4 subnet access prevented: {}", host);
                return Err(SandboxViolation::SsrfAttempt(raw_url.to_string()));
            }

            if let Some(rest) = host.strip_prefix("172.") {
                if let Some(second_octet) = rest.split('.').next().and_then(|o| o.parse::<u8>().ok()) {
                    if (16..=31).contains(&second_octet) {
                        warn!("Private Class B IPv4 subnet access prevented: {}", host);
                        return Err(SandboxViolation::SsrfAttempt(raw_url.to_string()));
                    }
                }
            }

            let clean_host = host.trim_start_matches('[').trim_end_matches(']');
            if let Ok(ip) = clean_host.parse::<IpAddr>() {
                let filter = Self::new();
                if !filter.is_ip_allowed(ip) {
                    return Err(SandboxViolation::SsrfAttempt(ip.to_string()));
                }
            }
        }

        // 5. Blocked domains check
        for blocked in &policy.blocked_domains {
            if blocked == "*" {
                return Err(SandboxViolation::BlockedDomain(raw_url.to_string()));
            }
            let pattern = blocked.trim_start_matches("*.");
            if lower.contains(pattern) {
                return Err(SandboxViolation::BlockedDomain(raw_url.to_string()));
            }
        }

        // 6. Allowed domains check
        if !policy.allowed_domains.is_empty() {
            let mut allowed = false;
            for allow in &policy.allowed_domains {
                if allow == "*" {
                    allowed = true;
                    break;
                }
                let pattern = allow.trim_start_matches("*.");
                if lower.contains(pattern) {
                    allowed = true;
                    break;
                }
            }
            if !allowed {
                return Err(SandboxViolation::BlockedDomain(raw_url.to_string()));
            }
        }

        Ok(())
    }

    /// Extracts the hostname/domain from a raw URL.
    pub fn extract_host(url: &str) -> Option<String> {
        let without_scheme = if let Some(pos) = url.find("://") {
            &url[pos + 3..]
        } else {
            url
        };

        // 1. Strip path/query/fragment
        let path_start = without_scheme
            .find(|c| c == '/' || c == '?' || c == '#')
            .unwrap_or(without_scheme.len());
        let authority = &without_scheme[..path_start];

        // 2. Strip userinfo (user:pass@)
        let host_port = if let Some(at_pos) = authority.rfind('@') {
            &authority[at_pos + 1..]
        } else {
            authority
        };

        // 3. Handle IPv6 bracketed host [::1]:8080
        if host_port.starts_with('[') {
            if let Some(close_bracket) = host_port.find(']') {
                let host_inside = &host_port[1..close_bracket];
                return Some(host_inside.to_string());
            }
        }

        // 4. Strip port (:8080)
        let host = host_port.split(':').next()?;
        if host.is_empty() {
            None
        } else {
            Some(host.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cloud_metadata_rejection() {
        let filter = SsrfFilter::new();
        assert!(filter.validate_url("http://169.254.169.254/latest/meta-data").is_err());
        assert!(filter.validate_url("http://169.254.169.250/metadata").is_err());
        assert!(filter.validate_url("http://metadata.google.internal/computeMetadata/v1").is_err());
        assert!(filter.validate_url("http://metadata.google/computeMetadata").is_err());
    }

    #[test]
    fn test_cgnat_and_rfc1918() {
        let filter = SsrfFilter::new();
        assert!(!filter.is_ip_allowed("100.64.0.1".parse().unwrap()));
        assert!(!filter.is_ip_allowed("100.127.255.254".parse().unwrap()));
        assert!(!filter.is_ip_allowed("10.0.0.1".parse().unwrap()));
        assert!(!filter.is_ip_allowed("172.16.0.1".parse().unwrap()));
        assert!(!filter.is_ip_allowed("192.168.1.1".parse().unwrap()));
        assert!(filter.is_ip_allowed("1.1.1.1".parse().unwrap()));
        assert!(filter.is_ip_allowed("8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn test_ipv6_special() {
        let filter = SsrfFilter::new();
        assert!(!filter.is_ip_allowed("::1".parse().unwrap()));
        assert!(!filter.is_ip_allowed("fe80::1".parse().unwrap()));
        assert!(!filter.is_ip_allowed("fc00::1".parse().unwrap()));
        assert!(!filter.is_ip_allowed("fd00::1".parse().unwrap()));
        assert!(!filter.is_ip_allowed("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!filter.is_ip_allowed("::ffff:169.254.169.254".parse().unwrap()));
        assert!(filter.is_ip_allowed("2606:4700:4700::1111".parse().unwrap()));
    }

    #[test]
    fn test_extract_host_variations() {
        assert_eq!(
            SsrfFilter::extract_host("https://user:pass@api.github.com:443/repos"),
            Some("api.github.com".to_string())
        );
        assert_eq!(
            SsrfFilter::extract_host("http://[::1]:8080/metrics"),
            Some("::1".to_string())
        );
        assert_eq!(
            SsrfFilter::extract_host("http://localhost:3000"),
            Some("localhost".to_string())
        );
    }
}
