//! Process Token Integrity Level and User Interface Privilege Isolation (UIPI) Detection.
//!
//! Windows UIPI silently discards PostMessage and SendInput events sent from a lower
//! integrity level process to a higher integrity level process (e.g. Medium to High).
//! This module inspects process token security labels to detect elevation disparities
//! and return structured, fail-closed refusals.

use crate::types::CuaError;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, FALSE, HANDLE, HWND};
#[cfg(windows)]
use windows_sys::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TokenIntegrityLevel,
    TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

/// Windows Mandatory Integrity Control RIDs
pub const SECURITY_MANDATORY_UNTRUSTED_RID: u32 = 0x0000;
pub const SECURITY_MANDATORY_LOW_RID: u32 = 0x1000;
pub const SECURITY_MANDATORY_MEDIUM_RID: u32 = 0x2000;
pub const SECURITY_MANDATORY_MEDIUM_PLUS_RID: u32 = 0x2100;
pub const SECURITY_MANDATORY_HIGH_RID: u32 = 0x3000;
pub const SECURITY_MANDATORY_SYSTEM_RID: u32 = 0x4000;
pub const SECURITY_MANDATORY_PROTECTED_PROCESS_RID: u32 = 0x5000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrityLevel {
    Untrusted,
    Low,
    Medium,
    MediumPlus,
    High,
    System,
    Protected,
    Unknown(u32),
}

impl IntegrityLevel {
    pub fn from_rid(rid: u32) -> Self {
        if rid < SECURITY_MANDATORY_LOW_RID {
            IntegrityLevel::Untrusted
        } else if rid < SECURITY_MANDATORY_MEDIUM_RID {
            IntegrityLevel::Low
        } else if rid < SECURITY_MANDATORY_MEDIUM_PLUS_RID {
            IntegrityLevel::Medium
        } else if rid < SECURITY_MANDATORY_HIGH_RID {
            IntegrityLevel::MediumPlus
        } else if rid < SECURITY_MANDATORY_SYSTEM_RID {
            IntegrityLevel::High
        } else if rid < SECURITY_MANDATORY_PROTECTED_PROCESS_RID {
            IntegrityLevel::System
        } else {
            IntegrityLevel::Protected
        }
    }
}

/// Cached integrity level of the current running LIVA agent process.
static AGENT_INTEGRITY: OnceLock<IntegrityLevel> = OnceLock::new();

/// Retrieves the IntegrityLevel of the current process.
#[cfg(windows)]
pub fn get_agent_integrity_level() -> IntegrityLevel {
    *AGENT_INTEGRITY.get_or_init(|| {
        unsafe {
            let mut h_token: HANDLE = 0;
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut h_token) == 0 {
                return IntegrityLevel::Medium; // Safe fallback
            }

            let level = query_token_integrity(h_token).unwrap_or(IntegrityLevel::Medium);
            CloseHandle(h_token);
            level
        }
    })
}

#[cfg(not(windows))]
pub fn get_agent_integrity_level() -> IntegrityLevel {
    IntegrityLevel::Medium
}

/// Queries the TokenIntegrityLevel from an open process token handle.
#[cfg(windows)]
unsafe fn query_token_integrity(h_token: HANDLE) -> Result<IntegrityLevel, CuaError> {
    let mut len: u32 = 0;
    // Initial call to get required buffer size
    GetTokenInformation(
        h_token,
        TokenIntegrityLevel,
        std::ptr::null_mut(),
        0,
        &mut len,
    );
    if len == 0 {
        return Err(CuaError::Win32Error(
            "GetTokenInformation size query failed".into(),
        ));
    }

    let mut buf = vec![0u8; len as usize];
    if GetTokenInformation(
        h_token,
        TokenIntegrityLevel,
        buf.as_mut_ptr() as *mut _,
        len,
        &mut len,
    ) == 0
    {
        return Err(CuaError::Win32Error(
            "GetTokenInformation buffer retrieval failed".into(),
        ));
    }

    let label = &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL);
    let sid = label.Label.Sid;
    if sid.is_null() {
        return Err(CuaError::Win32Error(
            "Null SID in TOKEN_MANDATORY_LABEL".into(),
        ));
    }

    let count_ptr = GetSidSubAuthorityCount(sid);
    if count_ptr.is_null() {
        return Err(CuaError::Win32Error(
            "GetSidSubAuthorityCount returned null".into(),
        ));
    }
    let count = *count_ptr;
    if count == 0 {
        return Err(CuaError::Win32Error(
            "Zero sub-authorities in token SID".into(),
        ));
    }

    let rid_ptr = GetSidSubAuthority(sid, (count - 1) as u32);
    if rid_ptr.is_null() {
        return Err(CuaError::Win32Error(
            "GetSidSubAuthority returned null".into(),
        ));
    }
    let rid = *rid_ptr;

    Ok(IntegrityLevel::from_rid(rid))
}

/// Queries the IntegrityLevel of a target process given its PID.
#[cfg(windows)]
pub fn get_process_integrity_level(pid: u32) -> Result<IntegrityLevel, CuaError> {
    unsafe {
        let h_proc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
        if h_proc == 0 {
            // Access denied on OpenProcess is a strong indicator that target is elevated / High integrity
            return Ok(IntegrityLevel::High);
        }

        let mut h_token: HANDLE = 0;
        let token_ok = OpenProcessToken(h_proc, TOKEN_QUERY, &mut h_token);
        CloseHandle(h_proc);

        if token_ok == 0 {
            return Ok(IntegrityLevel::High);
        }

        let result = query_token_integrity(h_token);
        CloseHandle(h_token);
        result
    }
}

#[cfg(not(windows))]
pub fn get_process_integrity_level(_pid: u32) -> Result<IntegrityLevel, CuaError> {
    Ok(IntegrityLevel::Medium)
}

/// Verdict returned by UIPI pre-flight verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UipiVerdict {
    pub is_blocked: bool,
    pub target_pid: u32,
    pub target_integrity: IntegrityLevel,
    pub agent_integrity: IntegrityLevel,
    pub refusal_message: Option<String>,
}

/// Evaluates whether an input action directed at `target_hwnd` would be blocked by UIPI.
pub fn check_uipi_restriction(target_hwnd: u64) -> Result<UipiVerdict, CuaError> {
    #[cfg(windows)]
    {
        let win_hwnd = target_hwnd as usize as HWND;
        unsafe {
            let mut target_pid: u32 = 0;
            GetWindowThreadProcessId(win_hwnd, &mut target_pid);

            if target_pid == 0 {
                return Err(CuaError::WindowNotFound(target_hwnd));
            }

            // If target process is our own process, input is never blocked by UIPI
            if target_pid == GetCurrentProcessId() {
                let own_level = get_agent_integrity_level();
                return Ok(UipiVerdict {
                    is_blocked: false,
                    target_pid,
                    target_integrity: own_level,
                    agent_integrity: own_level,
                    refusal_message: None,
                });
            }

            let agent_integrity = get_agent_integrity_level();
            let target_integrity = get_process_integrity_level(target_pid)?;

            if target_integrity > agent_integrity {
                let msg = format!(
                    "UIPI restriction: Target window (HWND {:#x}, PID {}) runs at {:?}, which is higher than agent process ({:?}). Windows silently drops synthetic input to elevated processes.",
                    target_hwnd, target_pid, target_integrity, agent_integrity
                );
                Ok(UipiVerdict {
                    is_blocked: true,
                    target_pid,
                    target_integrity,
                    agent_integrity,
                    refusal_message: Some(msg),
                })
            } else {
                Ok(UipiVerdict {
                    is_blocked: false,
                    target_pid,
                    target_integrity,
                    agent_integrity,
                    refusal_message: None,
                })
            }
        }
    }

    #[cfg(not(windows))]
    {
        Ok(UipiVerdict {
            is_blocked: false,
            target_pid: 0,
            target_integrity: IntegrityLevel::Medium,
            agent_integrity: IntegrityLevel::Medium,
            refusal_message: None,
        })
    }
}
