use rodio::{Sink, buffer::SamplesBuffer};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct TtsAudioPlayer {
    sink: Option<Arc<Sink>>,
    stop_id: Arc<AtomicUsize>,
    lock: Arc<Mutex<()>>,
}

/// Lấy khoá, phục hồi được cả khi mutex đã bị poison.
///
/// `lock` là `Mutex<()>` — nó chỉ tuần tự hoá các lời gọi `rodio::Sink`, KHÔNG
/// giữ dữ liệu nào. Poison nghĩa là một luồng đã panic khi đang giữ khoá, nhưng
/// ở đây không có bất biến nào để mà hỏng: từ chối phục vụ chỉ biến **một** sự
/// cố thành **câm vĩnh viễn** — mọi `play`/`stop`/`is_empty` sau đó cùng panic,
/// trong khi LIVA vẫn nhận lệnh và vẫn sinh câu trả lời. Nên lấy lại ruột và đi
/// tiếp.
///
/// Cùng cách xử lý, cùng lý lẽ với `messaging::outbox::khoa` — và với
/// `governor.rs`, `mcp::client`, `llm::tool_calling`. Trước bản vá này
/// `tts/audio.rs` là ngoại lệ duy nhất nằm trên đường phát tiếng mặc định.
fn guard(lock: &Mutex<()>) -> std::sync::MutexGuard<'_, ()> {
    lock.lock().unwrap_or_else(|e| e.into_inner())
}

impl TtsAudioPlayer {
    pub fn new(sink: Option<Arc<Sink>>) -> Self {
        Self {
            sink,
            stop_id: Arc::new(AtomicUsize::new(0)),
            lock: Arc::new(Mutex::new(())),
        }
    }

    /// Play at the Kokoro-native 24 kHz (kept for existing callers).
    pub fn play(&self, samples: Vec<f32>) -> usize {
        self.play_with_rate(samples, 24000)
    }

    /// Play mono f32 samples at an explicit sample rate (Piper voices are
    /// 22.05 kHz, Kokoro is 24 kHz — rodio resamples per source).
    pub fn play_with_rate(&self, samples: Vec<f32>, sample_rate: u32) -> usize {
        let _guard = guard(&self.lock);
        let val = self.stop_id.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(ref sink) = self.sink {
            sink.set_volume(1.0);
            let source = SamplesBuffer::new(1, sample_rate, samples);
            sink.append(source);
        }
        val
    }

    pub async fn stop(&self) {
        if let Some(ref sink) = self.sink {
            let active_id = {
                let _guard = guard(&self.lock);
                self.stop_id.fetch_add(1, Ordering::SeqCst) + 1
            };
            let sink_clone = Arc::clone(sink);
            let stop_id_clone = Arc::clone(&self.stop_id);
            let lock_clone = Arc::clone(&self.lock);

            tokio::spawn(async move {
                {
                    let _guard = guard(&lock_clone);
                    if sink_clone.empty() {
                        sink_clone.stop();
                        sink_clone.set_volume(1.0);
                        return;
                    }
                }

                // 5ms fade-out to prevent clicks/pops
                for i in (0..=20).rev() {
                    {
                        let _guard = guard(&lock_clone);
                        if stop_id_clone.load(Ordering::SeqCst) != active_id {
                            return;
                        }
                        sink_clone.set_volume(i as f32 / 20.0);
                    }
                    tokio::time::sleep(std::time::Duration::from_micros(250)).await;
                }

                {
                    let _guard = guard(&lock_clone);
                    if stop_id_clone.load(Ordering::SeqCst) == active_id {
                        sink_clone.stop();
                        sink_clone.set_volume(1.0);
                    }
                }
            });
        }
    }

    pub fn get_stop_id(&self) -> usize {
        self.stop_id.load(Ordering::SeqCst)
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        if let Some(ref sink) = self.sink {
            let _guard = guard(&self.lock);
            sink.empty()
        } else {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_audio_player_concurrency() {
        let player = TtsAudioPlayer::new(None);
        player.play(vec![0.0; 100]);
        player.stop().await;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(player.is_empty());
    }

    /// HỒI QUY — một panic khi đang giữ khoá KHÔNG được làm LIVA câm vĩnh viễn.
    ///
    /// `lock` là `Mutex<()>`: nó chỉ tuần tự hoá các lời gọi `rodio::Sink`, KHÔNG
    /// giữ dữ liệu, nên poison không làm hỏng bất biến nào. Nhưng cả sáu điểm
    /// khoá đều `.unwrap()`, nên một panic duy nhất bên trong vùng khoá — ví dụ
    /// `SamplesBuffer::new` assert `sample_rate != 0` (rodio-0.17.3
    /// `src/buffer.rs:43`) — biến sự cố một lần thành **câm vĩnh viễn**: mọi
    /// `play`/`stop`/`is_empty` sau đó đều panic cho tới khi khởi động lại tiến
    /// trình. LIVA vẫn nhận lệnh, vẫn sinh câu trả lời, chỉ là không nói nữa.
    ///
    /// Vì sao chưa test nào bắt được: nhánh chạm rodio chỉ chạy khi `sink` là
    /// `Some`, mà mọi test/e2e đều dựng `TtsAudioPlayer::new(None)`.
    ///
    /// Với `sink = None`, chỉ `play_with_rate` mới thật sự lấy khoá — `stop` và
    /// `is_empty` lấy khoá **bên trong** `if let Some(ref sink)`. Hai lời gọi
    /// cuối vì thế là chốt chặn cho tương lai: nếu ai đó chuyển khoá ra ngoài
    /// nhánh đó, test này phải bắt được ngay.
    #[tokio::test]
    async fn poison_khoa_khong_duoc_lam_liva_cam_vinh_vien() {
        let player = TtsAudioPlayer::new(None);

        let lock = Arc::clone(&player.lock);
        let ket_qua = std::thread::spawn(move || {
            let _g = lock.lock().unwrap();
            panic!("mo phong mot panic xay ra KHI DANG giu khoa");
        })
        .join();
        assert!(ket_qua.is_err(), "luong phu phai panic that");
        assert!(
            player.lock.is_poisoned(),
            "tien de cua test: khoa phai dang bi poison"
        );

        let truoc = player.get_stop_id();
        let id = player.play(vec![0.0; 32]);
        assert_eq!(id, truoc + 1, "play phai van chay va van tang stop_id");
        assert_eq!(id, player.get_stop_id());

        player.stop().await;
        let _ = player.is_empty();
    }

    #[tokio::test]
    async fn test_audio_player_stop_id() {
        let player = TtsAudioPlayer::new(None);
        let id0 = player.get_stop_id();
        let new_id = player.play(vec![0.0; 100]);
        let id1 = player.get_stop_id();
        assert_eq!(id1, id0 + 1);
        assert_eq!(new_id, id1);
    }
}
