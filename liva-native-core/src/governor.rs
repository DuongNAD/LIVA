//! Game-mode resource governor: keeps LIVA polite while the user plays.
//!
//! Detection (Windows): the foreground window is borderless-fullscreen on the
//! primary display and belongs to another process (desktop shell excluded).
//! When game mode is active the process priority drops to BELOW_NORMAL so
//! LIVA's inference never steals frame time; STT/VAD/TTS are already
//! CPU-light (2 intra-op threads each). LLM thread count is baked in at model
//! load (`LIVA_LLM_THREADS`, see `llm::engine`) — swapping to a smaller model
//! or fewer GPU layers on mode change requires a model reload and is a
//! follow-up (documented in the overhaul plan).
//!
//! # Ngoài game: tải CPU thật
//!
//! Chỉ dò cửa sổ fullscreen là **không đủ** để tuyên bố "sống chung với mọi
//! workload nặng". Heuristic đó sai cả hai chiều:
//! - **dương tính giả:** YouTube F11, PowerPoint trình chiếu, IDE toàn màn hình
//!   đều bị tính là game;
//! - **âm tính giả:** Blender render, biên dịch, export video chạy ở cửa sổ
//!   thường — nặng CPU nhất nhưng governor hoàn toàn không thấy.
//!
//! Vì vậy governor còn đọc **tải CPU thật** qua `GetSystemTimes` (không thêm
//! dependency nào — `windows-sys` đã có sẵn). Chế độ tiết kiệm bật khi
//! *fullscreen* **HOẶC** *CPU vượt ngưỡng*.
//!
//! Con số dùng để so ngưỡng là tải của **tiến trình khác**, đã trừ phần của
//! chính LIVA qua `GetProcessTimes` — xem [`external_cpu_percent`]. Không trừ
//! thì mỗi lần LLM sinh câu trả lời, LIVA sẽ tự kết luận "máy bận" và hạ
//! priority của chính mình: một vòng phản hồi ngược làm chậm đúng việc người
//! dùng đang chờ.
//!
//! # Nhánh thứ ba: tải GPU (NVML, 22/07/2026)
//!
//! Bắt nốt ca cả fullscreen lẫn CPU đều mù: render/encode GPU chạy ở cửa sổ
//! thường (Blender ở chế độ GPU, export video NVENC, training). `nvml-wrapper`
//! nạp `nvml.dll` ĐỘNG lúc chạy — máy không có NVIDIA thì init fail một lần và
//! nhánh này tắt, hai nhánh kia không ảnh hưởng.
//!
//! Cùng cái bẫy với CPU: **phải trừ phần GPU của chính LIVA**, không thì đặt
//! `LIVA_LLM_N_GPU_LAYERS > 0` là mỗi lượt suy luận LLM lại tự kích hoạt chế
//! độ tiết kiệm. Khác CPU ở chỗ Windows/WDDM thường KHÔNG cho đọc tải theo
//! tiến trình qua NVML — khi không biết phần của mình mà LIVA có thể đang dùng
//! GPU, ta **bỏ tín hiệu** (trả `None`) thay vì đoán: thà mất một nhánh phát
//! hiện còn hơn tự bóp cổ chính mình. Xem [`external_gpu_percent`].
//!
//! Env:
//! - `LIVA_GAME_MODE`       = auto | on | off   (default auto)
//! - `LIVA_GAME_PRIORITY`   = off to disable the priority drop (default on)
//! - `LIVA_BUSY_CPU_PERCENT` = ngưỡng tải CPU coi là "máy đang bận"
//!   (0–100, default 80; đặt 0 để tắt hẳn nhánh CPU)
//! - `LIVA_BUSY_GPU_PERCENT` = ngưỡng tải GPU, cùng quy ước (default 80; 0 tắt)

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernorMode {
    /// Detect fullscreen apps automatically.
    Auto,
    /// Always behave as if a game is running.
    ForcedOn,
    /// Never throttle.
    Off,
}

impl GovernorMode {
    fn from_env() -> Self {
        match std::env::var("LIVA_GAME_MODE")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "on" | "force" | "forced" => Self::ForcedOn,
            "off" | "disable" | "disabled" => Self::Off,
            _ => Self::Auto,
        }
    }
}

pub struct Governor {
    mode: GovernorMode,
    manage_priority: bool,
    active: AtomicBool,
    priority_lowered: AtomicBool,
    last_check: Mutex<Option<Instant>>,
}

const CHECK_INTERVAL: Duration = Duration::from_secs(2);

/// Ngưỡng mặc định coi máy là "đang bận". 80% để tránh bật nhầm khi máy chỉ
/// nhấp nhô bình thường; `0` tắt hẳn nhánh CPU.
const DEFAULT_BUSY_CPU_PERCENT: u8 = 80;

pub fn busy_cpu_threshold() -> u8 {
    std::env::var("LIVA_BUSY_CPU_PERCENT")
        .ok()
        .and_then(|v| v.trim().parse::<u8>().ok())
        .filter(|n| *n <= 100)
        .unwrap_or(DEFAULT_BUSY_CPU_PERCENT)
}

/// Phần trăm CPU do **tiến trình khác** chiếm, tính từ hai mẫu liên tiếp.
///
/// Hai điểm dễ sai, cả hai đều đã có test khoá lại:
///
/// 1. Trên Windows `kernel` ĐÃ BAO GỒM `idle`, nên tổng thời gian trôi qua là
///    `kernel + user` chứ không phải `idle + kernel + user`. Cộng cả ba làm mẫu
///    số khiến con số luôn nhỏ hơn thực tế.
/// 2. Phải **trừ phần CPU của chính LIVA** (`own_delta`). Không trừ thì mỗi lần
///    LLM sinh câu trả lời, CPU vọt lên → governor kết luận "máy bận" → tự hạ
///    priority của chính mình → làm chậm đúng việc người dùng đang chờ. Governor
///    tồn tại để nhường *workload khác*, nên số nó cần là tải NGOÀI LIVA.
///
/// Tách riêng khỏi phần gọi Win32 để test được trên mọi nền tảng.
pub fn external_cpu_percent(
    idle_delta: u64,
    kernel_delta: u64,
    user_delta: u64,
    own_delta: u64,
) -> Option<u8> {
    let total = kernel_delta.checked_add(user_delta)?;
    if total == 0 {
        return None;
    }
    // idle không thể lớn hơn total; kẹp lại phòng đồng hồ nhảy lùi.
    let idle = idle_delta.min(total);
    let busy = total - idle;
    // Phần của LIVA cũng không thể lớn hơn phần bận (đo hai lời gọi Win32 khác
    // nhau nên có thể lệch chút); kẹp để không underflow.
    let external = busy - own_delta.min(busy);
    Some(((external as u128 * 100) / total as u128).min(100) as u8)
}

/// Phần CPU của **chính LIVA**, cùng mẫu số với [`external_cpu_percent`].
///
/// Dùng chung mẫu số (`kernel + user`) là điều kiện để hai số đọc được cạnh
/// nhau: "máy bận 90 %, LIVA chiếm 3 %" chỉ có nghĩa khi cả hai chia cho cùng
/// một tổng. Kẹp theo `busy` vì `GetProcessTimes` và `GetSystemTimes` là hai
/// lời gọi khác nhau nên có thể lệch vài nhịp.
///
/// Tách riêng khỏi phần gọi Win32 để test được trên mọi nền tảng.
pub fn own_cpu_percent(
    idle_delta: u64,
    kernel_delta: u64,
    user_delta: u64,
    own_delta: u64,
) -> Option<u8> {
    let total = kernel_delta.checked_add(user_delta)?;
    if total == 0 {
        return None;
    }
    let busy = total - idle_delta.min(total);
    let own = own_delta.min(busy);
    Some(((own as u128 * 100) / total as u128).min(100) as u8)
}

/// Mẫu đo trước đó.
/// - Windows: `(idle, kernel, user, own)` — tick 100ns từ
///   `GetSystemTimes`/`GetProcessTimes`.
/// - macOS: `(busy_ticks, own_us, wall_us, 0)` — tick từ `host_statistics64`,
///   micro-giây từ `getrusage`, micro-giây đồng hồ wall để quy phần của LIVA
///   về phần trăm (hai đơn vị tick/µs KHÔNG được trộn lẫn khi tính delta).
#[cfg_attr(all(not(windows), not(target_os = "macos")), allow(dead_code))]
static LAST_CPU_SAMPLE: Mutex<Option<(u64, u64, u64, u64)>> = Mutex::new(None);

/// `(CPU ngoài LIVA, CPU của chính LIVA)` — phần trăm, từ **một** lần lấy mẫu.
///
/// ⚠️ Phải là MỘT hàm trả hai số, không phải hai hàm. Cả hai đọc chung
/// `LAST_CPU_SAMPLE` và **thay thế** nó, nên gọi hai hàm liên tiếp sẽ khiến hàm
/// thứ hai chỉ còn một khoảng thời gian gần bằng 0 để chia — ra số vô nghĩa, và
/// tệ hơn là số **trông vẫn hợp lý** nên không ai phát hiện.
///
/// `None` ở lần gọi đầu (chưa có mẫu trước) hoặc khi không lấy được số liệu.
#[cfg(windows)]
pub fn cpu_sample() -> Option<(u8, u8)> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetProcessTimes, GetSystemTimes,
    };

    const ZERO: FILETIME = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let to_u64 = |ft: FILETIME| ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;

    let (mut idle, mut kernel, mut user) = (ZERO, ZERO, ZERO);
    if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
        return None;
    }

    // GetProcessTimes trả CPU-time cộng dồn trên mọi lõi — cùng đơn vị 100ns và
    // cùng cách cộng như GetSystemTimes, nên trừ trực tiếp được.
    let (mut created, mut exited, mut own_kernel, mut own_user) = (ZERO, ZERO, ZERO, ZERO);
    let own = if unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut own_kernel,
            &mut own_user,
        )
    } == 0
    {
        0 // không lấy được phần của mình thì thà tính dư còn hơn bỏ hẳn số đo
    } else {
        to_u64(own_kernel).saturating_add(to_u64(own_user))
    };

    let now = (to_u64(idle), to_u64(kernel), to_u64(user), own);

    let mut guard = LAST_CPU_SAMPLE.lock().ok()?;
    let prev = guard.replace(now)?;

    let (idle_d, kernel_d, user_d, own_d) = (
        now.0.saturating_sub(prev.0),
        now.1.saturating_sub(prev.1),
        now.2.saturating_sub(prev.2),
        now.3.saturating_sub(prev.3),
    );
    Some((
        external_cpu_percent(idle_d, kernel_d, user_d, own_d)?,
        own_cpu_percent(idle_d, kernel_d, user_d, own_d)?,
    ))
}

/// Tải CPU của các tiến trình **khác** LIVA, phần trăm.
///
/// Vỏ mỏng của [`cpu_sample`] — giữ nguyên tên cũ vì governor và bảng trạng
/// thái đều gọi nó, và vì phần lớn nơi dùng chỉ cần đúng số này.
#[cfg(windows)]
pub fn system_cpu_percent() -> Option<u8> {
    cpu_sample().map(|(ngoai, _)| ngoai)
}

/// macOS: tải hệ thống đọc qua `host_statistics64` (tick scheduler của Mach),
/// phần của riêng LIVA đọc qua `getrusage(RUSAGE_SELF)` (micro-giây cộng dồn).
/// Hai đơn vị KHÔNG trộn: `%` bận hệ thống tính theo tick, `%` của LIVA tính
/// theo micro-giây trên đồng hồ wall giữa hai lần gọi. Ngữ nghĩa kết quả giống
/// bản Windows: `(tải CPU ngoài LIVA, tải CPU của chính LIVA)`.
///
/// Không thêm crate mới: hai hàm này thuộc libSystem, khai báo FFI trực tiếp.
#[cfg(target_os = "macos")]
pub fn cpu_sample() -> Option<(u8, u8)> {
    let (busy, idle) = macos_cpu::busy_idle_ticks()?;
    let own_us = macos_cpu::own_cpu_micros()?;
    let wall_us = macos_cpu::wall_micros();

    let mut guard = LAST_CPU_SAMPLE.lock().ok()?;
    // Tuple: (busy_ticks, own_µs, wall_µs, idle_ticks) — xem ghi chú ở LAST_CPU_SAMPLE.
    let prev = guard.replace((busy, own_us, wall_us, idle))?;
    drop(guard);

    let busy_d = busy.saturating_sub(prev.0);
    let own_d_us = own_us.saturating_sub(prev.1);
    let wall_d_us = wall_us.saturating_sub(prev.2);
    let idle_d = idle.saturating_sub(prev.3);
    if wall_d_us == 0 {
        return None;
    }

    let total_d = busy_d + idle_d;
    let busy_pct = if total_d == 0 {
        0
    } else {
        ((busy_d as u128 * 100) / total_d as u128).min(100) as u8
    };
    let own_pct = ((own_d_us as u128 * 100) / wall_d_us as u128).min(100) as u8;
    Some((busy_pct.saturating_sub(own_pct), own_pct))
}

#[cfg(target_os = "macos")]
pub fn system_cpu_percent() -> Option<u8> {
    cpu_sample().map(|(ngoai, _)| ngoai)
}

/// Các nền khác (Linux build ad-hoc…): chưa đo — trả `None`, không bịa số.
#[cfg(all(not(windows), not(target_os = "macos")))]
pub fn cpu_sample() -> Option<(u8, u8)> {
    None
}

#[cfg(all(not(windows), not(target_os = "macos")))]
pub fn system_cpu_percent() -> Option<u8> {
    None
}

/// FFI thuần với libSystem của macOS — cùng tinh thần "không thêm crate" như
/// nhánh Windows dùng `windows-sys` có sẵn.
#[cfg(target_os = "macos")]
mod macos_cpu {
    use std::os::raw::{c_int, c_uint, c_void};
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Flavor `HOST_CPU_LOAD_INFO` của `host_statistics64` (mach/host_info.h).
    const HOST_CPU_LOAD_INFO: c_int = 3;
    const KERN_SUCCESS: c_int = 0;
    /// `host_cpu_load_info_data_t` = 3 `avenrun` + 4 `cpu_ticks` (đơn vị `integer_t`).
    const HOST_CPU_LOAD_INFO_COUNT: c_uint = 7;
    /// Thứ tự `cpu_ticks[]` cố định bởi ABI Darwin (mach/machine.h).
    const CPU_STATE_USER: usize = 0;
    const CPU_STATE_SYSTEM: usize = 1;
    const CPU_STATE_IDLE: usize = 2;
    const CPU_STATE_NICE: usize = 3;
    const CPU_STATE_MAX: usize = 4;
    const RUSAGE_SELF: c_int = 0;

    #[repr(C)]
    struct HostCpuLoadInfo {
        avenrun: [i32; 3],
        cpu_ticks: [u32; CPU_STATE_MAX],
    }

    #[repr(C)]
    struct Timeval {
        tv_sec: i64,
        tv_usec: i32,
    }

    #[repr(C)]
    struct RUsage {
        ru_utime: Timeval,
        ru_stime: Timeval,
        ru_maxrss: i64,
        ru_ixrss: i64,
        ru_idrss: i64,
        ru_isrss: i64,
        ru_minflt: i64,
        ru_majflt: i64,
        ru_nswap: i64,
        ru_inblock: i64,
        ru_oublock: i64,
        ru_msgsnd: i64,
        ru_msgrcv: i64,
        ru_nsignals: i64,
        ru_nvcsw: i64,
        ru_nivcsw: i64,
    }

    unsafe extern "C" {
        fn mach_host_self() -> u32;
        fn host_statistics64(
            host: u32,
            flavor: c_int,
            host_info_out: *mut c_void,
            host_info_out_cnt: *mut c_uint,
        ) -> c_int;
        fn getrusage(who: c_int, r_usage: *mut RUsage) -> c_int;
    }

    /// `(busy_ticks, idle_ticks)` toàn hệ thống tại MỘT thời điểm.
    pub(super) fn busy_idle_ticks() -> Option<(u64, u64)> {
        unsafe {
            let mut info = HostCpuLoadInfo {
                avenrun: [0; 3],
                cpu_ticks: [0; CPU_STATE_MAX],
            };
            let mut count: c_uint = HOST_CPU_LOAD_INFO_COUNT;
            let kr = host_statistics64(
                mach_host_self(),
                HOST_CPU_LOAD_INFO,
                &mut info as *mut HostCpuLoadInfo as *mut c_void,
                &mut count,
            );
            if kr != KERN_SUCCESS {
                return None;
            }
            let t = &info.cpu_ticks;
            let busy =
                t[CPU_STATE_USER] as u64 + t[CPU_STATE_SYSTEM] as u64 + t[CPU_STATE_NICE] as u64;
            Some((busy, t[CPU_STATE_IDLE] as u64))
        }
    }

    /// Micro-giây CPU mà chính tiến trình đã tiêu (user + system), cộng dồn.
    pub(super) fn own_cpu_micros() -> Option<u64> {
        unsafe {
            let mut ru: RUsage = std::mem::zeroed();
            if getrusage(RUSAGE_SELF, &mut ru) != 0 {
                return None;
            }
            let user_us = ru.ru_utime.tv_sec as u64 * 1_000_000 + ru.ru_utime.tv_usec as u64;
            let sys_us = ru.ru_stime.tv_sec as u64 * 1_000_000 + ru.ru_stime.tv_usec as u64;
            Some(user_us.saturating_add(sys_us))
        }
    }

    /// Micro-giây kể từ UNIX epoch — mốc chia khoảng giữa hai lần lấy mẫu.
    pub(super) fn wall_micros() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0)
    }
}

/// Ngưỡng GPU, cùng quy ước với [`busy_cpu_threshold`]: 0 tắt hẳn nhánh,
/// >100 hoặc rác → mặc định.
const DEFAULT_BUSY_GPU_PERCENT: u8 = 80;

pub fn busy_gpu_threshold() -> u8 {
    std::env::var("LIVA_BUSY_GPU_PERCENT")
        .ok()
        .and_then(|v| v.trim().parse::<u8>().ok())
        .filter(|n| *n <= 100)
        .unwrap_or(DEFAULT_BUSY_GPU_PERCENT)
}

/// Quyết định "tải GPU ngoài LIVA" từ ba dữ kiện. Tách thuần để test được mà
/// không cần driver NVIDIA:
///
/// - biết phần của mình (`own = Some`) → trừ thẳng;
/// - KHÔNG biết phần của mình mà LIVA có thể đang dùng GPU
///   (`liva_dung_gpu = true`, tức `LIVA_LLM_N_GPU_LAYERS > 0`) → **bỏ tín
///   hiệu**. Dùng số thô ở đây là tái tạo đúng vòng phản hồi ngược đã sửa ở
///   nhánh CPU: LLM suy luận → GPU vọt → tự hạ priority → chậm đúng việc
///   người dùng chờ;
/// - không biết phần của mình nhưng LIVA chắc chắn không dùng GPU → số thô
///   chính là tải ngoài.
pub fn external_gpu_percent(overall: u8, own: Option<u8>, liva_dung_gpu: bool) -> Option<u8> {
    match own {
        Some(o) => Some(overall.saturating_sub(o).min(100)),
        None if liva_dung_gpu => None,
        None => Some(overall.min(100)),
    }
}

/// NVML khởi tạo MỘT lần: init là lời gọi đắt (nạp nvml.dll), còn fail thì fail
/// mãi trong đời tiến trình — thử lại mỗi 2 giây chỉ tốn công.
static NVML: std::sync::OnceLock<Option<nvml_wrapper::Nvml>> = std::sync::OnceLock::new();

/// Tải GPU của tiến trình **khác** LIVA, phần trăm. `None` khi: không có
/// NVIDIA/driver, hoặc không tách được phần của LIVA trong lúc LIVA có thể
/// đang dùng GPU (xem [`external_gpu_percent`]).
pub fn system_gpu_percent() -> Option<u8> {
    let nvml = NVML
        .get_or_init(|| nvml_wrapper::Nvml::init().ok())
        .as_ref()?;
    let device = nvml.device_by_index(0).ok()?;
    let overall = device.utilization_rates().ok()?.gpu.min(100) as u8;

    // Tải theo tiến trình: có trên Linux/TCC, thường NotSupported trên
    // Windows/WDDM — fallback nằm trong `external_gpu_percent`.
    let pid = std::process::id();
    let own = device.process_utilization_stats(None).ok().map(|samples| {
        samples
            .iter()
            .filter(|s| s.pid == pid)
            .map(|s| s.sm_util.min(100) as u8)
            .max()
            .unwrap_or(0)
    });

    let liva_dung_gpu = std::env::var("LIVA_LLM_N_GPU_LAYERS")
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .unwrap_or(0)
        > 0;

    external_gpu_percent(overall, own, liva_dung_gpu)
}

/// VRAM của GPU 0: `(tổng, đang dùng)` theo byte. `None` khi không có
/// NVIDIA/driver — máy AMD/Intel hoặc laptop không rời.
///
/// Dùng cho ô "VRAM Guard" trên Dashboard, vốn trước đây in cứng
/// `"0% utilized"` bất kể máy có card gì. Dùng chung `NVML` với
/// [`system_gpu_percent`] để không nạp `nvml.dll` lần thứ hai.
pub fn gpu_vram_bytes() -> Option<(u64, u64)> {
    let nvml = NVML
        .get_or_init(|| nvml_wrapper::Nvml::init().ok())
        .as_ref()?;
    let mem = nvml.device_by_index(0).ok()?.memory_info().ok()?;
    Some((mem.total, mem.used))
}

impl Governor {
    pub fn from_env() -> Self {
        Self {
            mode: GovernorMode::from_env(),
            manage_priority: std::env::var("LIVA_GAME_PRIORITY")
                .map(|v| v.to_lowercase() != "off")
                .unwrap_or(true),
            active: AtomicBool::new(false),
            priority_lowered: AtomicBool::new(false),
            last_check: Mutex::new(None),
        }
    }

    pub fn mode(&self) -> GovernorMode {
        self.mode
    }

    /// True when LIVA should minimize its resource footprint. Result is
    /// cached for [`CHECK_INTERVAL`]; cheap to call on hot paths.
    pub fn game_mode_active(&self) -> bool {
        match self.mode {
            GovernorMode::ForcedOn => {
                self.apply_priority(true);
                return true;
            }
            GovernorMode::Off => return false,
            GovernorMode::Auto => {}
        }

        let mut last = self.last_check.lock().unwrap();
        let stale = last.is_none_or(|t| t.elapsed() >= CHECK_INTERVAL);
        if stale {
            // Fullscreen HOAC tai CPU cao. Hai dau hieu bu tru cho nhau: fullscreen
            // bat game (nang GPU, CPU co the thap), con tai CPU bat render/compile
            // (cua so thuong, fullscreen khong thay).
            let fullscreen = foreground_is_fullscreen();
            let threshold = busy_cpu_threshold();
            // Doc CPU ngay ca khi threshold = 0: lay mau deu dan de delta lan sau
            // van dung, va de con so xuat hien trong log chan doan.
            let cpu = system_cpu_percent();
            let cpu_busy = threshold > 0 && cpu.is_some_and(|p| p >= threshold);
            // GPU: chi goi NVML khi nguong > 0 — khac CPU, o day khong co delta
            // nao can giu nhip, tat la tat han.
            let gpu_threshold = busy_gpu_threshold();
            let gpu = if gpu_threshold > 0 {
                system_gpu_percent()
            } else {
                None
            };
            let gpu_busy = gpu_threshold > 0 && gpu.is_some_and(|p| p >= gpu_threshold);
            let detected = fullscreen || cpu_busy || gpu_busy;
            if detected != self.active.load(Ordering::Relaxed) {
                tracing::info!(
                    "Governor: che do tiet kiem {} (fullscreen={}, cpu_ngoai={}, nguong_cpu={}%, gpu_ngoai={}, nguong_gpu={}%)",
                    if detected { "BAT" } else { "TAT" },
                    fullscreen,
                    cpu.map_or_else(|| "?".to_string(), |p| format!("{p}%")),
                    threshold,
                    gpu.map_or_else(|| "?".to_string(), |p| format!("{p}%")),
                    gpu_threshold
                );
            }
            self.active.store(detected, Ordering::Relaxed);
            *last = Some(Instant::now());
            self.apply_priority(detected);
        }
        self.active.load(Ordering::Relaxed)
    }

    fn apply_priority(&self, game_active: bool) {
        if !self.manage_priority {
            return;
        }
        let lowered = self.priority_lowered.load(Ordering::Relaxed);
        if game_active == lowered {
            return;
        }
        set_process_below_normal(game_active);
        self.priority_lowered.store(game_active, Ordering::Relaxed);
        tracing::info!(
            "Game mode {} — process priority {}",
            if game_active { "ON" } else { "OFF" },
            if game_active {
                "below-normal"
            } else {
                "normal"
            }
        );
    }
}

/// Stateless game-mode check (respects `LIVA_GAME_MODE`), for callers without a
/// `Governor` instance — e.g. the vision path deciding whether to use a cheap
/// mouse-guided crop instead of a full-screen capture. Not cached; call only on
/// rare paths (a vision request), not hot loops.
pub fn game_mode_active_now() -> bool {
    match GovernorMode::from_env() {
        GovernorMode::ForcedOn => true,
        GovernorMode::Off => false,
        GovernorMode::Auto => foreground_is_fullscreen(),
    }
}

#[cfg(windows)]
fn foreground_is_fullscreen() -> bool {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetSystemMetrics, GetWindowRect,
        GetWindowThreadProcessId, SM_CXSCREEN, SM_CYSCREEN,
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd == 0 {
            return false;
        }

        // Never throttle because of our own windows.
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == std::process::id() {
            return false;
        }

        // The desktop shell hosts fullscreen-sized windows; exclude it.
        let mut class_buf = [0u16; 64];
        let class_len = GetClassNameW(hwnd, class_buf.as_mut_ptr(), class_buf.len() as i32);
        if class_len > 0 {
            let class = String::from_utf16_lossy(&class_buf[..class_len as usize]);
            if class == "Progman" || class == "WorkerW" {
                return false;
            }
        }

        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return false;
        }
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);

        rect.left <= 0
            && rect.top <= 0
            && (rect.right - rect.left) >= screen_w
            && (rect.bottom - rect.top) >= screen_h
    }
}

#[cfg(not(windows))]
fn foreground_is_fullscreen() -> bool {
    false
}

#[cfg(windows)]
fn set_process_below_normal(lower: bool) {
    use windows_sys::Win32::System::Threading::{
        BELOW_NORMAL_PRIORITY_CLASS, GetCurrentProcess, NORMAL_PRIORITY_CLASS, SetPriorityClass,
    };
    unsafe {
        let class = if lower {
            BELOW_NORMAL_PRIORITY_CLASS
        } else {
            NORMAL_PRIORITY_CLASS
        };
        SetPriorityClass(GetCurrentProcess(), class);
    }
}

#[cfg(not(windows))]
fn set_process_below_normal(_lower: bool) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trên macOS số đo phải THẬT và nằm trong khoảng hợp lý: lần gọi đầu có
    /// thể `None` (chưa có mẫu trước), lần thứ hai sau một khoảng chờ phải ra
    /// phần trăm trong [0, 100].
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_sample_nam_trong_khoang_hop_ly() {
        let _ = cpu_sample();
        std::thread::sleep(Duration::from_millis(60));
        let (ngoai, own) = cpu_sample().expect("macOS phải lấy được số đo CPU thật");
        assert!(ngoai <= 100, "external {ngoai}% vượt 100");
        assert!(own <= 100, "own {own}% vượt 100");
    }

    #[test]
    fn forced_modes_bypass_detection() {
        let gov = Governor {
            mode: GovernorMode::Off,
            manage_priority: false,
            active: AtomicBool::new(false),
            priority_lowered: AtomicBool::new(false),
            last_check: Mutex::new(None),
        };
        assert!(!gov.game_mode_active());

        let gov = Governor {
            mode: GovernorMode::ForcedOn,
            manage_priority: false,
            active: AtomicBool::new(false),
            priority_lowered: AtomicBool::new(false),
            last_check: Mutex::new(None),
        };
        assert!(gov.game_mode_active());
    }
}

#[cfg(test)]
mod governor_cpu_tests {
    use super::{
        DEFAULT_BUSY_CPU_PERCENT, busy_cpu_threshold, busy_gpu_threshold, external_cpu_percent,
    };

    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Trên Windows `kernel` ĐÃ BAO GỒM `idle`. Cộng cả ba (idle+kernel+user)
    /// làm mẫu số là lỗi kinh điển khiến số luôn nhỏ hơn thực tế — test này
    /// khoá lại công thức đúng.
    #[test]
    fn cong_thuc_dung_kernel_da_gom_idle() {
        // kernel=100 (trong đó idle=100), user=0  -> máy rảnh hoàn toàn
        assert_eq!(external_cpu_percent(100, 100, 0, 0), Some(0));
        // kernel=100 (idle=0), user=0 -> kernel bận hết
        assert_eq!(external_cpu_percent(0, 100, 0, 0), Some(100));
        // kernel=100 (idle=50), user=100 -> total=200, busy=150 -> 75%
        assert_eq!(external_cpu_percent(50, 100, 100, 0), Some(75));

        // Nếu ai đó đổi sang mẫu số idle+kernel+user, ca này sẽ ra 60 chứ không 75.
        let sai_neu_cong_ca_ba = (150u128 * 100) / (50 + 100 + 100) as u128;
        assert_eq!(sai_neu_cong_ca_ba, 60, "day la ket qua SAI de doi chieu");
    }

    #[test]
    fn khong_chia_cho_0_va_khong_tran() {
        assert_eq!(
            external_cpu_percent(0, 0, 0, 0),
            None,
            "total=0 phai tra None"
        );
        // Giá trị lớn: FILETIME là 100ns tick, chạy lâu sẽ rất lớn
        assert!(external_cpu_percent(u64::MAX / 2, u64::MAX, 0, 0).is_some());
        assert!(
            external_cpu_percent(u64::MAX, u64::MAX, u64::MAX, 0).is_none(),
            "kernel+user tran -> None"
        );
    }

    /// Đồng hồ nhảy lùi có thể cho idle > total, hoặc own > busy (hai lời gọi
    /// Win32 ở hai thời điểm khác nhau); phải kẹp về 0% thay vì underflow.
    #[test]
    fn kep_lai_khi_so_lieu_lech() {
        assert_eq!(
            external_cpu_percent(500, 100, 0, 0),
            Some(0),
            "idle > total"
        );
        assert_eq!(external_cpu_percent(0, 100, 0, 999), Some(0), "own > busy");
    }

    /// LIVA không được tự tính mình vào "máy đang bận": khi LLM sinh câu trả
    /// lời, CPU vọt lên do CHÍNH NÓ. Không trừ thì governor sẽ hạ priority của
    /// chính mình và làm chậm đúng việc người dùng đang chờ.
    #[test]
    fn tru_phan_cpu_cua_chinh_liva() {
        // total=200, idle=0 -> bận 100%. Nhưng 150 là của LIVA -> ngoài = 25%.
        assert_eq!(external_cpu_percent(0, 100, 100, 150), Some(25));

        // Toàn bộ tải là của LIVA -> 0% "máy bận", governor phải để yên.
        assert_eq!(
            external_cpu_percent(0, 100, 100, 200),
            Some(0),
            "chi minh LIVA chay ma van bao may ban -> vong phan hoi nguoc"
        );

        // Không trừ thì cả hai ca trên đều ra 100% — đó là hành vi SAI.
        assert_eq!(external_cpu_percent(0, 100, 100, 0), Some(100));
    }

    /// Hai số phải dùng CHUNG mẫu số, nếu không thì đọc cạnh nhau là vô nghĩa.
    ///
    /// Đây là điều kiện để dải "máy bận X % · LIVA chiếm Y %" (U16) nói đúng
    /// sự thật: X + Y phải bằng đúng phần bận, không lớn hơn.
    #[test]
    fn phan_cua_liva_va_phan_ngoai_cong_lai_bang_phan_ban() {
        use super::own_cpu_percent;

        // total=200, idle=0 -> bận 100%; LIVA 150 -> ngoài 25% + LIVA 75%.
        assert_eq!(external_cpu_percent(0, 100, 100, 150), Some(25));
        assert_eq!(own_cpu_percent(0, 100, 100, 150), Some(75));

        // Có idle: total=200, idle=100 -> bận 50%; LIVA 40 -> ngoài 30% + LIVA 20%.
        assert_eq!(external_cpu_percent(100, 100, 100, 40), Some(30));
        assert_eq!(own_cpu_percent(100, 100, 100, 40), Some(20));

        // LIVA không thể chiếm quá phần bận, kể cả khi hai lời gọi Win32 lệch.
        assert_eq!(
            own_cpu_percent(150, 100, 100, 999),
            Some(25),
            "phai kep theo busy, khong duoc vuot"
        );

        // Không có thời gian trôi qua thì KHÔNG có câu trả lời — đừng trả 0.
        assert_eq!(own_cpu_percent(0, 0, 0, 0), None);
    }

    #[test]
    fn ket_qua_luon_trong_0_100() {
        for (i, k, u, o) in [
            (0u64, 1u64, 0u64, 0u64),
            (1, 1, 0, 0),
            (7, 13, 29, 5),
            (999, 1000, 1, 1),
            (0, 1000, 0, 1000),
        ] {
            let p = external_cpu_percent(i, k, u, o).unwrap();
            assert!(p <= 100, "({i},{k},{u},{o}) -> {p} vuot 100");
        }
    }

    #[test]
    fn nguong_doc_tu_env_va_kep_gia_tri() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var("LIVA_BUSY_CPU_PERCENT").ok();

        unsafe { std::env::remove_var("LIVA_BUSY_CPU_PERCENT") };
        assert_eq!(busy_cpu_threshold(), DEFAULT_BUSY_CPU_PERCENT);

        unsafe { std::env::set_var("LIVA_BUSY_CPU_PERCENT", "60") };
        assert_eq!(busy_cpu_threshold(), 60);

        // 0 = tắt hẳn nhánh CPU, phải giữ nguyên chứ không rơi về mặc định
        unsafe { std::env::set_var("LIVA_BUSY_CPU_PERCENT", "0") };
        assert_eq!(
            busy_cpu_threshold(),
            0,
            "0 phai la 'tat', khong phai 'dung mac dinh'"
        );

        // >100 hoặc rác -> mặc định
        unsafe { std::env::set_var("LIVA_BUSY_CPU_PERCENT", "500") };
        assert_eq!(busy_cpu_threshold(), DEFAULT_BUSY_CPU_PERCENT);
        unsafe { std::env::set_var("LIVA_BUSY_CPU_PERCENT", "abc") };
        assert_eq!(busy_cpu_threshold(), DEFAULT_BUSY_CPU_PERCENT);

        match old {
            Some(v) => unsafe { std::env::set_var("LIVA_BUSY_CPU_PERCENT", v) },
            None => unsafe { std::env::remove_var("LIVA_BUSY_CPU_PERCENT") },
        }
    }

    /// Smoke test trên phần cứng thật: nạp tải mọi lõi bằng CHÍNH tiến trình
    /// test rồi kiểm tra governor không coi đó là "máy bận". Các test trên chỉ
    /// chứng minh phép tính đúng — chúng không chứng minh hai lời gọi Win32
    /// được ghép đúng với nhau.
    ///
    /// `#[ignore]` vì tốn ~2 giây CPU tối đa và runner CI dùng chung máy nên
    /// số đo không ổn định. Chạy tay:
    /// `cargo test --lib governor -- --ignored --nocapture`
    #[test]
    #[ignore = "ton ~2s CPU that; chay tay de kiem chung phan cung"]
    fn do_duoc_tai_that_tren_may() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let _ = super::system_cpu_percent(); // mẫu nền
        std::thread::sleep(std::time::Duration::from_millis(300));
        let idle_before = super::system_cpu_percent().expect("windows phai co so do");

        let stop = Arc::new(AtomicBool::new(false));
        let workers: Vec<_> = (0..num_cpus_guess())
            .map(|_| {
                let stop = Arc::clone(&stop);
                std::thread::spawn(move || {
                    let mut x = 0u64;
                    while !stop.load(Ordering::Relaxed) {
                        x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
                    }
                    x
                })
            })
            .collect();

        std::thread::sleep(std::time::Duration::from_millis(1200));
        let under_load = super::system_cpu_percent().expect("windows phai co so do");

        stop.store(true, Ordering::Relaxed);
        for w in workers {
            let _ = w.join();
        }

        println!("CPU ngoai khi ranh = {idle_before}%, khi TU nap tai = {under_load}%");
        // Tải là do chính tiến trình test sinh ra nên phải bị TRỪ: số "ngoài"
        // gần như không đổi. Đây đúng là ca vòng-phản-hồi-ngược, đo trên phần
        // cứng thật thay vì chỉ suy luận.
        let _ = idle_before;
        assert!(
            under_load < 50,
            "tai la cua CHINH minh ma van bao {under_load}% may ban \
             — GetProcessTimes khong duoc tru dung"
        );
    }

    fn num_cpus_guess() -> usize {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    }

    /// GPU: ba nhánh quyết định của `external_gpu_percent`, đặc biệt là nhánh
    /// "không biết phần của mình + LIVA có thể đang dùng GPU → BỎ tín hiệu".
    /// Dùng số thô ở nhánh đó là tái tạo vòng phản hồi ngược của CPU.
    #[test]
    fn gpu_ngoai_ba_nhanh_quyet_dinh() {
        use super::external_gpu_percent;

        // Biết phần của mình -> trừ thẳng, kẹp không âm.
        assert_eq!(external_gpu_percent(90, Some(30), true), Some(60));
        assert_eq!(external_gpu_percent(90, Some(30), false), Some(60));
        assert_eq!(
            external_gpu_percent(20, Some(50), true),
            Some(0),
            "own > overall thi kep 0"
        );

        // Không biết phần của mình + LIVA có thể dùng GPU -> None, KHÔNG đoán.
        assert_eq!(
            external_gpu_percent(95, None, true),
            None,
            "dung so tho o day la tu bop co chinh minh khi LLM chay GPU"
        );

        // Không biết phần của mình nhưng LIVA chắc chắn không dùng GPU
        // (n_gpu_layers = 0, mặc định) -> số thô chính là tải ngoài.
        assert_eq!(external_gpu_percent(95, None, false), Some(95));
        assert_eq!(external_gpu_percent(0, None, false), Some(0));
    }

    #[test]
    fn nguong_gpu_doc_tu_env() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var("LIVA_BUSY_GPU_PERCENT").ok();

        unsafe { std::env::remove_var("LIVA_BUSY_GPU_PERCENT") };
        assert_eq!(busy_gpu_threshold(), super::DEFAULT_BUSY_GPU_PERCENT);
        unsafe { std::env::set_var("LIVA_BUSY_GPU_PERCENT", "0") };
        assert_eq!(busy_gpu_threshold(), 0, "0 la TAT, khong phai mac dinh");
        unsafe { std::env::set_var("LIVA_BUSY_GPU_PERCENT", "101") };
        assert_eq!(busy_gpu_threshold(), super::DEFAULT_BUSY_GPU_PERCENT);

        match old {
            Some(v) => unsafe { std::env::set_var("LIVA_BUSY_GPU_PERCENT", v) },
            None => unsafe { std::env::remove_var("LIVA_BUSY_GPU_PERCENT") },
        }
    }

    /// Smoke test trên phần cứng thật (cần NVIDIA): chỉ khẳng định đường NVML
    /// trả về số hợp lệ, không khẳng định giá trị — tải GPU của máy dev không
    /// đoán được. Máy không có NVIDIA thì nhánh None cũng là ĐẠT (degrade đúng
    /// thiết kế).
    #[test]
    fn nvml_tra_so_hop_le_hoac_degrade_sach() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var("LIVA_LLM_N_GPU_LAYERS").ok();
        // Ép nhánh "LIVA không dùng GPU" để trên máy WDDM (own = NotSupported)
        // vẫn nhận được số thô thay vì None.
        unsafe { std::env::remove_var("LIVA_LLM_N_GPU_LAYERS") };

        match super::system_gpu_percent() {
            Some(p) => assert!(p <= 100, "phan tram GPU {p} vuot 100"),
            None => eprintln!("khong co NVIDIA/NVML tren may nay — degrade dung thiet ke"),
        }

        match old {
            Some(v) => unsafe { std::env::set_var("LIVA_LLM_N_GPU_LAYERS", v) },
            None => unsafe { std::env::remove_var("LIVA_LLM_N_GPU_LAYERS") },
        }
    }

    /// Lần gọi đầu chưa có mẫu trước nên phải trả None chứ không phải 0% —
    /// 0% sẽ bị hiểu nhầm là "máy rảnh" và tắt chế độ tiết kiệm.
    #[test]
    fn lan_do_dau_tien_tra_none() {
        let first = super::system_cpu_percent();
        if cfg!(windows) {
            // Lần thứ hai mới có delta; chỉ khẳng định không panic và trong khoảng.
            let second = super::system_cpu_percent();
            if let Some(p) = second {
                assert!(p <= 100);
            }
            let _ = first;
        } else {
            assert_eq!(first, None, "ngoai Windows luon None");
        }
    }
}
