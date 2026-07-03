#![allow(dead_code)]

use rodio::{Sink, buffer::SamplesBuffer};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct TtsAudioPlayer {
    sink: Option<Arc<Sink>>,
    stop_id: Arc<AtomicUsize>,
    lock: Arc<Mutex<()>>,
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
        let _guard = self.lock.lock().unwrap();
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
                let _guard = self.lock.lock().unwrap();
                self.stop_id.fetch_add(1, Ordering::SeqCst) + 1
            };
            let sink_clone = Arc::clone(sink);
            let stop_id_clone = Arc::clone(&self.stop_id);
            let lock_clone = Arc::clone(&self.lock);

            tokio::spawn(async move {
                {
                    let _guard = lock_clone.lock().unwrap();
                    if sink_clone.empty() {
                        sink_clone.stop();
                        sink_clone.set_volume(1.0);
                        return;
                    }
                }

                // 5ms fade-out to prevent clicks/pops
                for i in (0..=20).rev() {
                    {
                        let _guard = lock_clone.lock().unwrap();
                        if stop_id_clone.load(Ordering::SeqCst) != active_id {
                            return;
                        }
                        sink_clone.set_volume(i as f32 / 20.0);
                    }
                    tokio::time::sleep(std::time::Duration::from_micros(250)).await;
                }

                {
                    let _guard = lock_clone.lock().unwrap();
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
            let _guard = self.lock.lock().unwrap();
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
