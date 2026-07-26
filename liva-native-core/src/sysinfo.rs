//! Số đo hệ thống THẬT cho `get_system_status`.
//!
//! Vì sao có module này: bảng sức khoẻ trên Dashboard trước đây trả **số cứng**
//! — `cpuUsage: 12`, `totalMemory: 16e9`, `uptime: 3600`, `rssMemory: 100_000_000`,
//! `voiceEngine.latencyMs: 5` — và UI poll nó 3 giây một lần để vẽ 8 đèn xanh.
//! Người dùng nhìn thấy một hệ thống khoẻ mạnh bất kể nó có khoẻ hay không.
//! Điều đó đi ngược đúng nguyên tắc dự án đã dựng ở chỗ khác ("báo trung thực
//! thay vì thành công giả").
//!
//! Quy ước của cả module: **`None` là một câu trả lời hợp lệ.** Không đoán, không
//! điền số mặc định. Máy không phải Windows, không có NVIDIA, hoặc API trả lỗi →
//! `None` → JSON `null` → UI hiện `--`. Một ô trống nói thật có ích hơn một con
//! số đẹp nói dối.
//!
//! Không thêm crate mới: `windows-sys` đã có sẵn trong cây phụ thuộc với đúng
//! các feature cần (`Win32_System_ProcessStatus`, `Win32_System_Threading`,
//! `Win32_Foundation`). Chọn `GetPerformanceInfo` thay vì `GlobalMemoryStatusEx`
//! cũng vì lý do đó — cái sau nằm trong `Win32_System_SystemInformation`, feature
//! chưa bật, và bật thêm feature chỉ để lấy hai con số là không đáng.

/// Khoảng thời gian tiến trình đã chạy, tính bằng giây.
///
/// Lấy từ thời điểm TẠO TIẾN TRÌNH do OS ghi lại, không phải từ một mốc
/// `Instant` cắm lúc boot. Lý do: LIVA có **hai điểm vào** dựng `AppState` riêng
/// (gateway `main.rs` và vỏ Tauri `liva-desktop`), nên bất kỳ mốc nào phải cắm
/// bằng tay đều sẽ thiếu ở một trong hai đường — đúng loại trôi lệch mà bảng này
/// sinh ra để phát hiện.
#[cfg(windows)]
pub fn process_uptime_secs() -> Option<u64> {
    use std::time::{SystemTime, UNIX_EPOCH};
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    const ZERO: FILETIME = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut created, mut exited, mut kernel, mut user) = (ZERO, ZERO, ZERO, ZERO);
    if unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    } == 0
    {
        return None;
    }

    let ft_100ns = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
    let unix_100ns = ft_100ns.checked_sub(FILETIME_UNIX_EPOCH_100NS)?;
    let start_secs = unix_100ns / 10_000_000;
    let now_secs = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    now_secs.checked_sub(start_secs)
}

#[cfg(not(windows))]
pub fn process_uptime_secs() -> Option<u64> {
    None
}

/// FILETIME đếm từ 1601-01-01 theo đơn vị 100ns; UNIX epoch cách mốc đó
/// 11 644 473 600 giây. Tách hằng ra ngoài `cfg` để test được trên mọi nền.
pub const FILETIME_UNIX_EPOCH_100NS: u64 = 11_644_473_600 * 10_000_000;

/// RAM vật lý `(tổng, còn trống)` theo byte.
#[cfg(windows)]
pub fn ram_bytes() -> Option<(u64, u64)> {
    use windows_sys::Win32::System::ProcessStatus::{
        GetPerformanceInfo, PERFORMANCE_INFORMATION,
    };

    let mut info: PERFORMANCE_INFORMATION = unsafe { std::mem::zeroed() };
    info.cb = std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32;
    if unsafe { GetPerformanceInfo(&mut info, info.cb) } == 0 {
        return None;
    }
    // PhysicalTotal/PhysicalAvailable đếm theo TRANG, không phải byte.
    let page = info.PageSize as u64;
    Some((
        (info.PhysicalTotal as u64).saturating_mul(page),
        (info.PhysicalAvailable as u64).saturating_mul(page),
    ))
}

#[cfg(not(windows))]
pub fn ram_bytes() -> Option<(u64, u64)> {
    None
}

/// Bộ nhớ của chính tiến trình LIVA: `(working set, commit charge)` theo byte.
///
/// `working set` = RSS (phần đang nằm trong RAM vật lý). `commit charge`
/// (`PagefileUsage`) là con số thay cho "heap": Rust không có heap do runtime
/// quản lý nên không có gì để báo cáo dưới cái tên đó — báo commit charge và gọi
/// đúng tên nó ở tầng UI.
#[cfg(windows)]
pub fn process_memory_bytes() -> Option<(u64, u64)> {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let mut pmc: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    pmc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    if unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, pmc.cb) } == 0 {
        return None;
    }
    Some((pmc.WorkingSetSize as u64, pmc.PagefileUsage as u64))
}

#[cfg(not(windows))]
pub fn process_memory_bytes() -> Option<(u64, u64)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mốc chuyển FILETIME→UNIX phải đúng, nếu không uptime lệch hàng thế kỷ mà
    /// vẫn "có vẻ hợp lý" (một số u64 dương lớn).
    #[test]
    fn moc_filetime_unix_dung_bang_11644473600_giay() {
        assert_eq!(FILETIME_UNIX_EPOCH_100NS / 10_000_000, 11_644_473_600);
    }

    /// Trên Windows các số đo phải THẬT: uptime không âm và trong khoảng hợp lý,
    /// RAM tổng > 0 và phần trống không vượt tổng, RSS > 0.
    ///
    /// Trên nền khác, hợp đồng là `None` — không phải 0, không phải số bịa.
    #[cfg(windows)]
    #[test]
    fn so_do_windows_nam_trong_khoang_hop_ly() {
        let up = process_uptime_secs().expect("Windows phải lấy được uptime tiến trình");
        // Tiến trình test vừa chạy: vài giây tới vài giờ. Chặn trên 1 năm đủ để
        // bắt lỗi mốc epoch mà không đòi máy CI phải nhanh.
        assert!(up < 365 * 24 * 3600, "uptime {up}s — sai mốc FILETIME?");

        let (tong, trong) = ram_bytes().expect("Windows phải lấy được RAM");
        assert!(tong > 0, "RAM tổng phải > 0");
        assert!(trong <= tong, "RAM trống ({trong}) không được vượt tổng ({tong})");
        // 256 MB: thấp hơn mọi máy chạy nổi LIVA — bắt lỗi quên nhân PageSize.
        assert!(tong > 256 * 1024 * 1024, "RAM tổng {tong} B quá nhỏ — quên nhân PageSize?");

        let (rss, commit) = process_memory_bytes().expect("Windows phải lấy được RSS");
        assert!(rss > 0, "RSS phải > 0");
        assert!(commit > 0, "commit charge phải > 0");
    }

    #[cfg(not(windows))]
    #[test]
    fn ngoai_windows_tra_none_chu_khong_bia_so() {
        assert_eq!(process_uptime_secs(), None);
        assert_eq!(ram_bytes(), None);
        assert_eq!(process_memory_bytes(), None);
    }
}
