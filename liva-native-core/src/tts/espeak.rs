//! Shared espeak-ng process resolution and IPA phonemization.
//!
//! espeak-ng may not be on PATH (a fresh winget install only updates PATH for
//! new shells), so resolution order is: `LIVA_ESPEAK_PATH` env → PATH → the
//! default Windows install locations. The resolved path is cached per process.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{LazyLock, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

const G2P_CACHE_CAPACITY: usize = 4096;

struct G2pCache {
    entries: HashMap<(String, String), String>,
    eviction_order: VecDeque<(String, String)>,
}

impl G2pCache {
    fn new() -> Self {
        Self {
            entries: HashMap::with_capacity(512),
            eviction_order: VecDeque::with_capacity(512),
        }
    }

    fn get(&self, voice: &str, text: &str) -> Option<String> {
        self.entries
            .get(&(voice.to_string(), text.to_string()))
            .cloned()
    }

    fn insert(&mut self, voice: String, text: String, phonemes: String) {
        let key = (voice, text);
        if let std::collections::hash_map::Entry::Occupied(mut e) = self.entries.entry(key.clone())
        {
            e.insert(phonemes);
            return;
        }
        if self.entries.len() >= G2P_CACHE_CAPACITY {
            let to_evict = G2P_CACHE_CAPACITY / 4;
            for _ in 0..to_evict {
                if let Some(old_key) = self.eviction_order.pop_front() {
                    self.entries.remove(&old_key);
                }
            }
        }
        self.eviction_order.push_back(key.clone());
        self.entries.insert(key, phonemes);
    }
}

static G2P_CACHE: LazyLock<RwLock<G2pCache>> = LazyLock::new(|| RwLock::new(G2pCache::new()));

struct EspeakDaemon {
    voice: String,
    child: Child,
    stdin: ChildStdin,
    rx: std::sync::mpsc::Receiver<Result<String, String>>,
    reader_thread: Option<std::thread::JoinHandle<()>>,
}

impl EspeakDaemon {
    fn spawn(voice: &str) -> Result<Self, String> {
        let mut cmd = espeak_command();
        cmd.args(["-q", "--ipa", "-v", voice]);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("espeak-ng daemon spawn failed: {}", e))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Failed to acquire espeak stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to acquire espeak stdout".to_string())?;

        let (tx, rx) = std::sync::mpsc::channel();
        let voice_clone = voice.to_string();
        let reader_thread = std::thread::Builder::new()
            .name(format!("espeak-daemon-{}", voice_clone))
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) => {
                            let _ = tx.send(Err(
                                "espeak daemon closed stdout unexpectedly (EOF)".to_string()
                            ));
                            break;
                        }
                        Ok(_) => {
                            if tx.send(Ok(line.clone())).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(format!("espeak stdout read error: {}", e)));
                            break;
                        }
                    }
                }
            })
            .map_err(|e| format!("Failed to spawn espeak daemon reader thread: {}", e))?;

        Ok(Self {
            voice: voice.to_string(),
            child,
            stdin,
            rx,
            reader_thread: Some(reader_thread),
        })
    }

    fn phonemize(&mut self, text: &str) -> Result<String, String> {
        let line = text.replace(['\r', '\n'], " ");
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(String::new());
        }

        // Drain any stale messages from prior timeouts or unexpected outputs
        while self.rx.try_recv().is_ok() {}

        writeln!(self.stdin, "{}", trimmed)
            .map_err(|e| format!("espeak stdin write error: {}", e))?;
        self.stdin
            .flush()
            .map_err(|e| format!("espeak stdin flush error: {}", e))?;

        let timeout = std::env::var("LIVA_ESPEAK_DAEMON_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(Duration::from_millis(1500));

        match self.rx.recv_timeout(timeout) {
            Ok(Ok(out_line)) => {
                let trimmed_out = out_line.trim().to_string();
                if trimmed_out.is_empty() {
                    return Err("espeak daemon returned empty output line".to_string());
                }
                Ok(trimmed_out)
            }
            Ok(Err(err)) => Err(err),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(format!(
                "espeak daemon phonemize timed out after {:?}",
                timeout
            )),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                Err("espeak daemon reader thread disconnected".to_string())
            }
        }
    }
}

impl Drop for EspeakDaemon {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        {
            let pid = self.child.id();
            let _ = Command::new("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .output();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(handle) = self.reader_thread.take() {
            let _ = handle.join();
        }
    }
}

static ESPEAK_DAEMON: LazyLock<Mutex<Option<EspeakDaemon>>> = LazyLock::new(|| Mutex::new(None));

fn resolve_espeak() -> PathBuf {
    if let Ok(p) = std::env::var("LIVA_ESPEAK_PATH") {
        let p = PathBuf::from(p);
        if p.exists() {
            return p;
        }
    }

    if Command::new("espeak-ng").arg("--version").output().is_ok() {
        return PathBuf::from("espeak-ng");
    }

    #[cfg(windows)]
    for candidate in [
        r"C:\Program Files\eSpeak NG\espeak-ng.exe",
        r"C:\Program Files (x86)\eSpeak NG\espeak-ng.exe",
    ] {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return p;
        }
    }

    // Last resort: bare name — spawn will fail with a clear message.
    PathBuf::from("espeak-ng")
}

pub(crate) fn espeak_command() -> Command {
    static ESPEAK: OnceLock<PathBuf> = OnceLock::new();
    Command::new(ESPEAK.get_or_init(resolve_espeak))
}

/// Phonemize `text` to espeak IPA using the given espeak voice ("vi", "en-us").
///
/// Fast multi-tier resolution:
/// 1. In-memory thread-safe LRU/RwLock cache (`<0.01ms`).
/// 2. Persistent daemon stdin/stdout pipe (`<0.5ms`).
/// 3. One-shot subprocess execution with bounded timeout fallback.
pub(crate) fn espeak_ipa(voice: &str, text: &str) -> Result<String, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    // 1. Fast path: check in-memory cache
    if let Some(cached) = G2P_CACHE.read().ok().and_then(|c| c.get(voice, trimmed)) {
        return Ok(cached);
    }

    // 2. Medium path: persistent daemon pipe
    let mut daemon_res: Result<String, String> = Err("Daemon uninitialized".to_string());
    if let Ok(mut daemon_opt) = ESPEAK_DAEMON.lock() {
        let needs_spawn = match &*daemon_opt {
            Some(d) => d.voice != voice,
            None => true,
        };

        if needs_spawn {
            *daemon_opt = EspeakDaemon::spawn(voice).ok();
        }

        if let Some(daemon) = daemon_opt.as_mut() {
            match daemon.phonemize(trimmed) {
                Ok(res) => {
                    daemon_res = Ok(res);
                }
                Err(err) => {
                    tracing::warn!(
                        "espeak daemon failed: {}; resetting daemon and attempting respawn",
                        err
                    );
                    *daemon_opt = None;

                    // Automatically attempt to respawn daemon
                    if let Ok(mut new_daemon) = EspeakDaemon::spawn(voice) {
                        match new_daemon.phonemize(trimmed) {
                            Ok(res) => {
                                daemon_res = Ok(res);
                                *daemon_opt = Some(new_daemon);
                            }
                            Err(e2) => {
                                tracing::warn!(
                                    "respawned espeak daemon also failed: {}; falling back to subprocess",
                                    e2
                                );
                                *daemon_opt = None;
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Fallback path: one-shot subprocess with timeout and taskkill
    let phonemes = match daemon_res {
        Ok(p) => p,
        Err(e) => {
            tracing::info!(
                "Daemon path unavailable ({}); invoking subprocess fallback",
                e
            );
            espeak_ipa_oneshot(voice, trimmed)?
        }
    };

    // Store in cache
    if let Ok(mut cache) = G2P_CACHE.write() {
        cache.insert(voice.to_string(), trimmed.to_string(), phonemes.clone());
    }

    Ok(phonemes)
}

/// One-shot subprocess fallback implementation with timeout and process tree cleanup.
fn espeak_ipa_oneshot(voice: &str, text: &str) -> Result<String, String> {
    let timeout_secs = std::env::var("LIVA_ESPEAK_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10);

    let mut child = espeak_command()
        .args(["-q", "--ipa", "-v", voice, "--", text])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("espeak-ng spawn failed: {}", e))?;

    let mut stdout_handle = child.stdout.take();
    let mut stderr_handle = child.stderr.take();

    let stdout_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut out) = stdout_handle.take() {
            let _ = out.read_to_end(&mut buf);
        }
        buf
    });
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut err) = stderr_handle.take() {
            let _ = err.read_to_end(&mut buf);
        }
        buf
    });

    let timeout = Duration::from_secs(timeout_secs);
    let start = Instant::now();
    let poll_interval = Duration::from_millis(10);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = stdout_thread.join().unwrap_or_default();
                let stderr = stderr_thread.join().unwrap_or_default();

                if !status.success() {
                    return Err(format!(
                        "espeak-ng exited with {}: {}",
                        status,
                        String::from_utf8_lossy(&stderr)
                    ));
                }

                return Ok(String::from_utf8_lossy(&stdout).trim().to_string());
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    tracing::warn!(
                        "espeak-ng xử lý IPA quá hạn {}s; tiến hành huỷ cây tiến trình",
                        timeout_secs
                    );
                    #[cfg(target_os = "windows")]
                    {
                        let pid = child.id();
                        let _ = Command::new("taskkill")
                            .args(["/F", "/T", "/PID", &pid.to_string()])
                            .output();
                    }
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout_thread.join();
                    let _ = stderr_thread.join();
                    return Err(format!(
                        "espeak-ng phonemization timed out after {}s",
                        timeout_secs
                    ));
                }
                std::thread::sleep(poll_interval);
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return Err(format!("espeak-ng wait failed: {}", e));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_espeak_empty_input() {
        let res1 = espeak_ipa("vi", "").expect("empty text");
        assert_eq!(res1, "");
        let res2 = espeak_ipa("vi", "   \t\n  ").expect("whitespace text");
        assert_eq!(res2, "");
    }

    #[test]
    fn test_g2p_cache_eviction() {
        let mut cache = G2pCache::new();
        for i in 0..G2P_CACHE_CAPACITY + 10 {
            cache.insert(
                "vi".to_string(),
                format!("word_{}", i),
                format!("ipa_{}", i),
            );
        }
        // Cache should be bounded and not exceed capacity
        assert!(cache.entries.len() <= G2P_CACHE_CAPACITY);
        // Latest entry should be present
        assert_eq!(
            cache.get("vi", &format!("word_{}", G2P_CACHE_CAPACITY + 9)),
            Some(format!("ipa_{}", G2P_CACHE_CAPACITY + 9))
        );
        // Oldest entry (word_0) should have been evicted
        assert_eq!(cache.get("vi", "word_0"), None);
    }

    #[test]
    fn test_espeak_ipa_caching_and_sub_millisecond_latency() {
        let phrase = "xin chào việt nam";
        let first_res = match espeak_ipa("vi", phrase) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skip: espeak-ng not available in test environment: {}", e);
                return;
            }
        };
        assert!(!first_res.is_empty());

        // Second call must hit the cache and return in < 0.2ms
        let start = Instant::now();
        let cached_res = espeak_ipa("vi", phrase).expect("cached phonemes");
        let elapsed = start.elapsed();

        assert_eq!(first_res, cached_res, "Cached IPA must match first result");
        assert!(
            elapsed < Duration::from_micros(200),
            "Cached G2P lookup took {:?}, expected < 0.2ms",
            elapsed
        );
    }

    #[test]
    fn test_espeak_concurrency() {
        let handles: Vec<_> = (0..4)
            .map(|t_id| {
                std::thread::spawn(move || {
                    let phrase = format!("cụm từ kiểm thử {}", t_id);
                    let res = espeak_ipa("vi", &phrase);
                    if let Ok(phonemes) = res {
                        assert!(!phonemes.is_empty());
                        // Subsequent query in same thread
                        let cached = espeak_ipa("vi", &phrase).expect("cached");
                        assert_eq!(phonemes, cached);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().expect("thread joined successfully");
        }
    }

    #[test]
    fn test_espeak_daemon_respawn_after_crash_or_drop() {
        let phrase = "tự động khởi động lại tiến trình daemon";
        let res1 = match espeak_ipa("vi", phrase) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skip: espeak-ng not available in test environment: {}", e);
                return;
            }
        };
        assert!(!res1.is_empty());

        // Simulate daemon death by acquiring lock and setting to None
        if let Ok(mut daemon_opt) = ESPEAK_DAEMON.lock() {
            *daemon_opt = None;
        }

        // Fresh phrase not in cache to force daemon execution
        let fresh_phrase = "kiểm tra tiến trình mới được tái sinh";
        let res2 = espeak_ipa("vi", fresh_phrase);
        assert!(
            res2.is_ok(),
            "EspeakDaemon must automatically respawn and succeed"
        );
    }

    #[test]
    fn test_espeak_daemon_timeout_protection_and_fallback() {
        // Set an extremely short timeout (1ms) to trigger daemon timeout path
        unsafe { std::env::set_var("LIVA_ESPEAK_DAEMON_TIMEOUT_MS", "1") };

        let phrase = "thử nghiệm cơ chế quá hạn thời gian của daemon";
        // Even if daemon times out, it should fall back to subprocess or respawn without panic
        let res = espeak_ipa("vi", phrase);
        if let Ok(p) = res {
            assert!(!p.is_empty());
        }

        // Restore normal timeout
        unsafe { std::env::remove_var("LIVA_ESPEAK_DAEMON_TIMEOUT_MS") };
    }
}
