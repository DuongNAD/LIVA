//! Lock-Free Single-Producer Single-Consumer (SPSC) Audio Ring Buffer for Real-Time DSP.
//!
//! Designed for zero lock contention, sub-millisecond transit latency (<10us intra-process),
//! cache-line aligned atomics (64-byte alignment) to eliminate false sharing, and zero jitter.
//! Supports PCM 16kHz/24kHz Float32 and i16 buffers (RFC-003 Milestone 3).

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

/// CPU Cache-line size in bytes on modern x86_64 and ARM64 architectures.
pub const CACHE_LINE_BYTES: usize = 64;

/// Default sample capacity for 16kHz audio (~2.048 seconds @ 16kHz mono).
pub const DEFAULT_AUDIO_BUFFER_CAPACITY: usize = 32768;

/// Cache-line aligned atomic integer wrapper to eliminate False Sharing between CPU cores.
#[repr(align(64))]
pub struct CacheAlignedAtomic {
    pub value: AtomicUsize,
    _pad: [u8; 56], // 64 - 8 bytes
}

impl CacheAlignedAtomic {
    pub const fn new(val: usize) -> Self {
        Self {
            value: AtomicUsize::new(val),
            _pad: [0u8; 56],
        }
    }
}

impl Clone for CacheAlignedAtomic {
    fn clone(&self) -> Self {
        Self::new(self.value.load(Ordering::Relaxed))
    }
}

impl std::fmt::Debug for CacheAlignedAtomic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CacheAlignedAtomic")
            .field("value", &self.value.load(Ordering::Relaxed))
            .finish()
    }
}

/// Static / Const-generic SPSC Lock-Free Audio Ring Buffer.
///
/// Guarantees:
/// - Const generic power-of-two capacity `CAP`.
/// - 64-byte cache-line aligned `head` and `tail` pointers.
/// - Branchless modulo operations via bitwise mask (`CAP - 1`).
/// - Zero allocations during runtime push/pop.
pub struct SpscRingBuffer<T, const CAP: usize> {
    buffer: UnsafeCell<[T; CAP]>,
    head: CacheAlignedAtomic, // Producer index
    tail: CacheAlignedAtomic, // Consumer index
}

unsafe impl<T: Send, const CAP: usize> Send for SpscRingBuffer<T, CAP> {}
unsafe impl<T: Send, const CAP: usize> Sync for SpscRingBuffer<T, CAP> {}

impl<T, const CAP: usize> SpscRingBuffer<T, CAP> {
    /// Total capacity in elements.
    #[inline(always)]
    pub const fn capacity(&self) -> usize {
        CAP
    }

    /// Bitwise mask for fast wrapping.
    #[inline(always)]
    const fn mask(&self) -> usize {
        CAP - 1
    }

    /// Number of elements currently available for reading.
    #[inline(always)]
    pub fn available_read(&self) -> usize {
        let head = self.head.value.load(Ordering::Acquire);
        let tail = self.tail.value.load(Ordering::Relaxed);
        head.wrapping_sub(tail)
    }

    /// Number of elements available for writing before filling the buffer.
    #[inline(always)]
    pub fn available_write(&self) -> usize {
        let head = self.head.value.load(Ordering::Relaxed);
        let tail = self.tail.value.load(Ordering::Acquire);
        CAP.saturating_sub(head.wrapping_sub(tail))
    }

    /// Returns true if the buffer contains no unread elements.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        let head = self.head.value.load(Ordering::Acquire);
        let tail = self.tail.value.load(Ordering::Relaxed);
        head == tail
    }

    /// Returns true if the buffer has no space remaining for writing.
    #[inline(always)]
    pub fn is_full(&self) -> bool {
        self.available_write() == 0
    }

    /// Skips up to `count` unread elements in the buffer.
    pub fn skip(&self, count: usize) -> usize {
        loop {
            let head = self.head.value.load(Ordering::Acquire);
            let tail = self.tail.value.load(Ordering::Acquire);
            let available = head.wrapping_sub(tail);
            let to_skip = count.min(available);
            if to_skip == 0 {
                return 0;
            }
            match self.tail.value.compare_exchange_weak(
                tail,
                tail.wrapping_add(to_skip),
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return to_skip,
                Err(_) => std::hint::spin_loop(),
            }
        }
    }

    /// Atomically discards all unread elements from the consumer side without modifying the producer's write pointer (`head`).
    ///
    /// This is lock-free and thread-safe to call during active streaming (e.g. barge-in preemption).
    ///
    /// Returns the number of discarded elements.
    #[inline]
    pub fn flush_consumer(&self) -> usize {
        loop {
            let head = self.head.value.load(Ordering::Acquire);
            let tail = self.tail.value.load(Ordering::Acquire);
            let discarded = head.wrapping_sub(tail);
            if discarded == 0 || discarded > CAP {
                return 0;
            }
            match self.tail.value.compare_exchange_weak(
                tail,
                head,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return discarded,
                Err(_) => std::hint::spin_loop(),
            }
        }
    }

    /// Alias for [`flush_consumer`](Self::flush_consumer).
    #[inline]
    pub fn clear_consumer(&self) -> usize {
        self.flush_consumer()
    }

    /// Resets the ring buffer to empty state by zeroing both head and tail pointers.
    ///
    /// # Concurrency Warning
    /// This method resets both producer (`head`) and consumer (`tail`) indices.
    /// It must ONLY be called when the buffer is quiescent / re-initialized (i.e. when no producer or consumer threads are active).
    /// For lock-free active streaming flushes (e.g. barge-in preemption), use [`flush_consumer`](Self::flush_consumer).
    pub fn clear(&self) {
        self.head.value.store(0, Ordering::Release);
        self.tail.value.store(0, Ordering::Release);
    }
}

impl<T: Copy + Default, const CAP: usize> SpscRingBuffer<T, CAP> {
    /// Creates a new const-generic `SpscRingBuffer`.
    ///
    /// # Panics
    /// Panics at compile/runtime if `CAP` is 0 or not a power of 2.
    pub fn new() -> Self {
        assert!(CAP > 0 && CAP.is_power_of_two(), "CAP must be a power of two");
        Self {
            buffer: UnsafeCell::new([T::default(); CAP]),
            head: CacheAlignedAtomic::new(0),
            tail: CacheAlignedAtomic::new(0),
        }
    }

    /// Pushes a slice of elements into the buffer.
    /// Returns the number of elements actually written (min(src.len(), available_write())).
    pub fn push_slice(&self, src: &[T]) -> usize {
        let to_write = src.len().min(self.available_write());
        if to_write == 0 {
            return 0;
        }

        let head = self.head.value.load(Ordering::Relaxed);
        let mask = self.mask();
        let offset = head & mask;

        let buf = unsafe { &mut *self.buffer.get() };

        let first_chunk = (CAP - offset).min(to_write);
        buf[offset..offset + first_chunk].copy_from_slice(&src[..first_chunk]);

        if first_chunk < to_write {
            let remainder = to_write - first_chunk;
            buf[..remainder].copy_from_slice(&src[first_chunk..to_write]);
        }

        self.head
            .value
            .store(head.wrapping_add(to_write), Ordering::Release);
        to_write
    }

    /// Pops elements from the buffer into the destination slice.
    /// Returns the number of elements actually read (min(dst.len(), available_read())).
    /// If a concurrent flush occurs during copy, the stale read is aborted and returns 0.
    pub fn pop_slice(&self, dst: &mut [T]) -> usize {
        let head = self.head.value.load(Ordering::Acquire);
        let tail = self.tail.value.load(Ordering::Acquire);
        let available = head.wrapping_sub(tail);
        let to_read = dst.len().min(available);
        if to_read == 0 {
            return 0;
        }

        let mask = self.mask();
        let offset = tail & mask;

        let buf = unsafe { &*self.buffer.get() };

        let first_chunk = (CAP - offset).min(to_read);
        dst[..first_chunk].copy_from_slice(&buf[offset..offset + first_chunk]);

        if first_chunk < to_read {
            let remainder = to_read - first_chunk;
            dst[first_chunk..to_read].copy_from_slice(&buf[..remainder]);
        }

        // Atomically commit the read only if tail has not been modified (e.g. flushed) concurrently
        match self.tail.value.compare_exchange(
            tail,
            tail.wrapping_add(to_read),
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => to_read,
            Err(_) => {
                // A concurrent flush occurred during the read!
                // The data copied is stale and was invalidated by the flush.
                // Do NOT advance tail and return 0.
                0
            }
        }
    }

    /// Peeks elements from the buffer into the destination slice without consuming them.
    pub fn peek_slice(&self, dst: &mut [T]) -> usize {
        let to_peek = dst.len().min(self.available_read());
        if to_peek == 0 {
            return 0;
        }

        let tail = self.tail.value.load(Ordering::Relaxed);
        let mask = self.mask();
        let offset = tail & mask;

        let buf = unsafe { &*self.buffer.get() };

        let first_chunk = (CAP - offset).min(to_peek);
        dst[..first_chunk].copy_from_slice(&buf[offset..offset + first_chunk]);

        if first_chunk < to_peek {
            let remainder = to_peek - first_chunk;
            dst[first_chunk..to_peek].copy_from_slice(&buf[..remainder]);
        }

        to_peek
    }
}

impl<T: Copy + Default, const CAP: usize> Default for SpscRingBuffer<T, CAP> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const CAP: usize> std::fmt::Debug for SpscRingBuffer<T, CAP> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpscRingBuffer")
            .field("capacity", &CAP)
            .field("available_read", &self.available_read())
            .field("available_write", &self.available_write())
            .finish()
    }
}

/// Dynamic Heap-Allocated Lock-Free SPSC Audio Ring Buffer.
///
/// Used when the buffer capacity is configured at runtime (e.g. sample-rate dependent).
pub struct AudioRingBuffer<T> {
    buffer_ptr: *mut T,
    capacity: usize,
    mask: usize,
    head: CacheAlignedAtomic,
    tail: CacheAlignedAtomic,
    underruns: AtomicU64,
    overruns: AtomicU64,
    total_written: AtomicU64,
    total_read: AtomicU64,
}

unsafe impl<T: Send> Send for AudioRingBuffer<T> {}
unsafe impl<T: Send> Sync for AudioRingBuffer<T> {}

impl<T> AudioRingBuffer<T> {
    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    #[inline(always)]
    pub fn available_read(&self) -> usize {
        let head = self.head.value.load(Ordering::Acquire);
        let tail = self.tail.value.load(Ordering::Relaxed);
        head.wrapping_sub(tail)
    }

    #[inline(always)]
    pub fn available_write(&self) -> usize {
        let head = self.head.value.load(Ordering::Relaxed);
        let tail = self.tail.value.load(Ordering::Acquire);
        self.capacity.saturating_sub(head.wrapping_sub(tail))
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        let head = self.head.value.load(Ordering::Acquire);
        let tail = self.tail.value.load(Ordering::Relaxed);
        head == tail
    }

    #[inline(always)]
    pub fn is_full(&self) -> bool {
        self.available_write() == 0
    }

    /// Skips (discards) up to `count` unread samples.
    pub fn skip(&self, count: usize) -> usize {
        loop {
            let head = self.head.value.load(Ordering::Acquire);
            let tail = self.tail.value.load(Ordering::Acquire);
            let available = head.wrapping_sub(tail);
            let to_skip = count.min(available);
            if to_skip == 0 {
                return 0;
            }
            match self.tail.value.compare_exchange_weak(
                tail,
                tail.wrapping_add(to_skip),
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    self.total_read.fetch_add(to_skip as u64, Ordering::Relaxed);
                    return to_skip;
                }
                Err(_) => {
                    std::hint::spin_loop();
                }
            }
        }
    }

    /// Atomically discards all unread samples from the consumer side without modifying the producer's write pointer (`head`).
    ///
    /// This is lock-free and thread-safe to call during active streaming (e.g. barge-in preemption).
    ///
    /// Returns the number of discarded samples.
    #[inline]
    pub fn flush_consumer(&self) -> usize {
        loop {
            let head = self.head.value.load(Ordering::Acquire);
            let tail = self.tail.value.load(Ordering::Acquire);
            let discarded = head.wrapping_sub(tail);
            if discarded == 0 || discarded > self.capacity {
                return 0;
            }
            match self.tail.value.compare_exchange_weak(
                tail,
                head,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    self.total_read.fetch_add(discarded as u64, Ordering::Relaxed);
                    return discarded;
                }
                Err(_) => {
                    std::hint::spin_loop();
                }
            }
        }
    }

    /// Alias for [`flush_consumer`](Self::flush_consumer).
    #[inline]
    pub fn clear_consumer(&self) -> usize {
        self.flush_consumer()
    }

    /// Resets the ring buffer pointers to zero.
    ///
    /// # Concurrency Warning
    /// This method resets both producer (`head`) and consumer (`tail`) indices.
    /// It must ONLY be called when the buffer is quiescent / re-initialized (i.e. when no producer or consumer threads are active).
    /// For lock-free active streaming flushes (e.g. barge-in preemption), use [`flush_consumer`](Self::flush_consumer).
    pub fn clear(&self) {
        self.head.value.store(0, Ordering::Release);
        self.tail.value.store(0, Ordering::Release);
    }

    /// Returns diagnostic counters: (underruns, overruns, total_written, total_read).
    pub fn metrics(&self) -> (u64, u64, u64, u64) {
        (
            self.underruns.load(Ordering::Relaxed),
            self.overruns.load(Ordering::Relaxed),
            self.total_written.load(Ordering::Relaxed),
            self.total_read.load(Ordering::Relaxed),
        )
    }
}

impl<T: Copy + Default> AudioRingBuffer<T> {
    /// Creates a new `AudioRingBuffer` with the specified power-of-two capacity.
    pub fn new(capacity: usize) -> Self {
        Self::try_new(capacity).expect("Failed to initialize AudioRingBuffer")
    }

    /// Tries to create a new `AudioRingBuffer` with power-of-two capacity.
    pub fn try_new(capacity: usize) -> Result<Self, String> {
        if capacity == 0 || !capacity.is_power_of_two() {
            return Err(format!(
                "Audio ring buffer capacity must be a power of 2, got {}",
                capacity
            ));
        }

        let size = capacity
            .checked_mul(std::mem::size_of::<T>())
            .ok_or_else(|| "Capacity overflow in byte calculation".to_string())?;
        let align = std::mem::align_of::<T>().max(CACHE_LINE_BYTES);
        let layout = Layout::from_size_align(size, align)
            .map_err(|e| format!("Invalid layout for AudioRingBuffer: {}", e))?;

        let buffer_ptr = unsafe { alloc_zeroed(layout) as *mut T };
        if buffer_ptr.is_null() {
            return Err("Memory allocation failed for AudioRingBuffer".to_string());
        }

        Ok(Self {
            buffer_ptr,
            capacity,
            mask: capacity - 1,
            head: CacheAlignedAtomic::new(0),
            tail: CacheAlignedAtomic::new(0),
            underruns: AtomicU64::new(0),
            overruns: AtomicU64::new(0),
            total_written: AtomicU64::new(0),
            total_read: AtomicU64::new(0),
        })
    }

    /// Pushes a slice of samples into the buffer.
    /// If `src.len() > available_write()`, only the available portion is written and overruns counter increments.
    pub fn push_slice(&self, src: &[T]) -> usize {
        let available = self.available_write();
        let to_write = src.len().min(available);

        if to_write < src.len() {
            self.overruns.fetch_add(1, Ordering::Relaxed);
        }
        if to_write == 0 {
            return 0;
        }

        let head = self.head.value.load(Ordering::Relaxed);
        let offset = head & self.mask;

        unsafe {
            let first_chunk = (self.capacity - offset).min(to_write);
            std::ptr::copy_nonoverlapping(src.as_ptr(), self.buffer_ptr.add(offset), first_chunk);

            if first_chunk < to_write {
                let remainder = to_write - first_chunk;
                std::ptr::copy_nonoverlapping(
                    src.as_ptr().add(first_chunk),
                    self.buffer_ptr,
                    remainder,
                );
            }
        }

        self.head
            .value
            .store(head.wrapping_add(to_write), Ordering::Release);
        self.total_written
            .fetch_add(to_write as u64, Ordering::Relaxed);
        to_write
    }

    /// Pops samples from the buffer into the destination slice.
    /// If `dst.len() > available_read()`, only the available portion is read and underruns counter increments.
    /// If a concurrent flush occurs during copy, the stale read is aborted and returns 0.
    pub fn pop_slice(&self, dst: &mut [T]) -> usize {
        let head = self.head.value.load(Ordering::Acquire);
        let tail = self.tail.value.load(Ordering::Acquire);
        let available = head.wrapping_sub(tail);
        let to_read = dst.len().min(available);

        if to_read < dst.len() {
            self.underruns.fetch_add(1, Ordering::Relaxed);
        }
        if to_read == 0 {
            return 0;
        }

        let offset = tail & self.mask;

        unsafe {
            let first_chunk = (self.capacity - offset).min(to_read);
            std::ptr::copy_nonoverlapping(self.buffer_ptr.add(offset), dst.as_mut_ptr(), first_chunk);

            if first_chunk < to_read {
                let remainder = to_read - first_chunk;
                std::ptr::copy_nonoverlapping(
                    self.buffer_ptr,
                    dst.as_mut_ptr().add(first_chunk),
                    remainder,
                );
            }
        }

        // Atomically commit the read only if tail has not been modified (e.g. flushed) concurrently
        match self.tail.value.compare_exchange(
            tail,
            tail.wrapping_add(to_read),
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                self.total_read.fetch_add(to_read as u64, Ordering::Relaxed);
                to_read
            }
            Err(_) => {
                // A concurrent flush occurred during the read!
                // The data copied is stale and was invalidated by the flush.
                // Do NOT advance tail and return 0.
                0
            }
        }
    }

    /// Peeks samples from the buffer without consuming them.
    pub fn peek_slice(&self, dst: &mut [T]) -> usize {
        let to_peek = dst.len().min(self.available_read());
        if to_peek == 0 {
            return 0;
        }

        let tail = self.tail.value.load(Ordering::Relaxed);
        let offset = tail & self.mask;

        unsafe {
            let first_chunk = (self.capacity - offset).min(to_peek);
            std::ptr::copy_nonoverlapping(self.buffer_ptr.add(offset), dst.as_mut_ptr(), first_chunk);

            if first_chunk < to_peek {
                let remainder = to_peek - first_chunk;
                std::ptr::copy_nonoverlapping(
                    self.buffer_ptr,
                    dst.as_mut_ptr().add(first_chunk),
                    remainder,
                );
            }
        }
        to_peek
    }
}

impl<T: Copy + Default> Default for AudioRingBuffer<T> {
    fn default() -> Self {
        Self::new(DEFAULT_AUDIO_BUFFER_CAPACITY)
    }
}

impl<T> Drop for AudioRingBuffer<T> {
    fn drop(&mut self) {
        if !self.buffer_ptr.is_null() && self.capacity > 0 {
            let size = self.capacity * std::mem::size_of::<T>();
            let align = std::mem::align_of::<T>().max(CACHE_LINE_BYTES);
            if let Ok(layout) = Layout::from_size_align(size, align) {
                unsafe {
                    dealloc(self.buffer_ptr as *mut u8, layout);
                }
            }
            self.buffer_ptr = std::ptr::null_mut();
        }
    }
}

impl<T> std::fmt::Debug for AudioRingBuffer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (underruns, overruns, total_written, total_read) = self.metrics();
        f.debug_struct("AudioRingBuffer")
            .field("capacity", &self.capacity)
            .field("available_read", &self.available_read())
            .field("available_write", &self.available_write())
            .field("underruns", &underruns)
            .field("overruns", &overruns)
            .field("total_written", &total_written)
            .field("total_read", &total_read)
            .finish()
    }
}

/// Specialized Float32 PCM Audio Ring Buffer.
pub type AudioRingBufferF32 = AudioRingBuffer<f32>;

/// Specialized Int16 PCM Audio Ring Buffer.
pub type AudioRingBufferI16 = AudioRingBuffer<i16>;

/// Full-Duplex Audio Ring Buffer pair managing simultaneous Capture (Mic In) and Playback (Speaker Out).
pub struct DuplexAudioRingBuffer {
    pub capture_ring: Arc<AudioRingBufferF32>,
    pub playback_ring: Arc<AudioRingBufferF32>,
    sample_rate_capture: u32,
    sample_rate_playback: u32,
}

impl DuplexAudioRingBuffer {
    /// Creates a full-duplex audio ring buffer with default 32768 capacity (2.048s @ 16kHz).
    pub fn new(sample_rate_capture: u32, sample_rate_playback: u32) -> Self {
        Self::with_capacity(DEFAULT_AUDIO_BUFFER_CAPACITY, sample_rate_capture, sample_rate_playback)
    }

    /// Creates a full-duplex audio ring buffer with custom power-of-two capacity.
    pub fn with_capacity(
        capacity: usize,
        sample_rate_capture: u32,
        sample_rate_playback: u32,
    ) -> Self {
        Self {
            capture_ring: Arc::new(AudioRingBufferF32::new(capacity)),
            playback_ring: Arc::new(AudioRingBufferF32::new(capacity)),
            sample_rate_capture,
            sample_rate_playback,
        }
    }

    #[inline(always)]
    pub fn sample_rate_capture(&self) -> u32 {
        self.sample_rate_capture
    }

    #[inline(always)]
    pub fn sample_rate_playback(&self) -> u32 {
        self.sample_rate_playback
    }

    /// Push microphone capture samples from WebSocket/AudioWorklet.
    #[inline(always)]
    pub fn push_capture(&self, samples: &[f32]) -> usize {
        self.capture_ring.push_slice(samples)
    }

    /// Pop microphone capture samples for DSP / VAD / STT pipeline.
    #[inline(always)]
    pub fn pop_capture(&self, dst: &mut [f32]) -> usize {
        self.capture_ring.pop_slice(dst)
    }

    /// Push synthesized speaker audio from TTS engine.
    #[inline(always)]
    pub fn push_playback(&self, samples: &[f32]) -> usize {
        self.playback_ring.push_slice(samples)
    }

    /// Pop speaker playback samples for DAC / WebRTC / WebSocket output.
    #[inline(always)]
    pub fn pop_playback(&self, dst: &mut [f32]) -> usize {
        self.playback_ring.pop_slice(dst)
    }

    /// Instant flush on Barge-In preemption.
    /// Uses lock-free consumer-side flush to safely discard unread playback audio
    /// without corrupting active producer indices during concurrent streaming.
    ///
    /// Returns the number of discarded playback samples.
    #[inline(always)]
    pub fn flush_playback(&self) -> usize {
        self.playback_ring.flush_consumer()
    }
}

impl std::fmt::Debug for DuplexAudioRingBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DuplexAudioRingBuffer")
            .field("sample_rate_capture", &self.sample_rate_capture)
            .field("sample_rate_playback", &self.sample_rate_playback)
            .field("capture_ring", &self.capture_ring)
            .field("playback_ring", &self.playback_ring)
            .finish()
    }
}

// ============================================================================
// Sample Format Conversion Utilities
// ============================================================================

/// Converts Float32 normalized samples [-1.0, 1.0] to Signed 16-bit PCM with clipping protection.
pub fn f32_to_i16_slice(src: &[f32], dst: &mut [i16]) -> usize {
    let count = src.len().min(dst.len());
    for i in 0..count {
        let clamped = src[i].clamp(-1.0, 1.0);
        dst[i] = if clamped >= 0.0 {
            (clamped * 32767.0).round() as i16
        } else {
            (clamped * 32768.0).round() as i16
        };
    }
    count
}

/// Converts Signed 16-bit PCM samples to Float32 normalized samples [-1.0, 1.0].
pub fn i16_to_f32_slice(src: &[i16], dst: &mut [f32]) -> usize {
    let count = src.len().min(dst.len());
    for i in 0..count {
        dst[i] = src[i] as f32 / 32768.0;
    }
    count
}

/// Fast linear resampler from 16kHz to 24kHz (ratio 3:2).
pub fn resample_linear_16k_to_24k(src: &[f32], dst: &mut Vec<f32>) {
    if src.is_empty() {
        return;
    }
    let target_len = (src.len() * 3) / 2;
    dst.clear();
    dst.reserve(target_len);

    for i in 0..target_len {
        let src_idx_f = (i as f32) * (2.0 / 3.0);
        let idx0 = (src_idx_f.floor() as usize).min(src.len() - 1);
        let idx1 = (idx0 + 1).min(src.len() - 1);
        let frac = src_idx_f - idx0 as f32;
        let sample = src[idx0] * (1.0 - frac) + src[idx1] * frac;
        dst.push(sample);
    }
}

/// Fast linear resampler from 24kHz to 16kHz (ratio 2:3).
pub fn resample_linear_24k_to_16k(src: &[f32], dst: &mut Vec<f32>) {
    if src.is_empty() {
        return;
    }
    let target_len = (src.len() * 2) / 3;
    dst.clear();
    dst.reserve(target_len);

    for i in 0..target_len {
        let src_idx_f = (i as f32) * (3.0 / 2.0);
        let idx0 = (src_idx_f.floor() as usize).min(src.len() - 1);
        let idx1 = (idx0 + 1).min(src.len() - 1);
        let frac = src_idx_f - idx0 as f32;
        let sample = src[idx0] * (1.0 - frac) + src[idx1] * frac;
        dst.push(sample);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::thread;

    #[test]
    fn test_spsc_ring_buffer_const_generic_operations() {
        let rb = SpscRingBuffer::<f32, 1024>::new();
        assert_eq!(rb.capacity(), 1024);
        assert_eq!(rb.available_read(), 0);
        assert_eq!(rb.available_write(), 1024);
        assert!(rb.is_empty());
        assert!(!rb.is_full());

        let input = vec![0.5f32; 256];
        let written = rb.push_slice(&input);
        assert_eq!(written, 256);
        assert_eq!(rb.available_read(), 256);
        assert_eq!(rb.available_write(), 768);

        let mut output = vec![0.0f32; 256];
        let read = rb.pop_slice(&mut output);
        assert_eq!(read, 256);
        assert_eq!(output, input);
        assert!(rb.is_empty());
    }

    #[test]
    fn test_spsc_ring_buffer_const_generic_peek_and_skip() {
        let rb = SpscRingBuffer::<f32, 128>::new();
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(rb.push_slice(&data), 5);

        let mut peek_buf = vec![0.0f32; 5];
        let peeked = rb.peek_slice(&mut peek_buf);
        assert_eq!(peeked, 5);
        assert_eq!(peek_buf, data);
        assert_eq!(rb.available_read(), 5);

        let skipped = rb.skip(2);
        assert_eq!(skipped, 2);
        assert_eq!(rb.available_read(), 3);

        let mut rest = vec![0.0f32; 3];
        assert_eq!(rb.pop_slice(&mut rest), 3);
        assert_eq!(rest, vec![3.0, 4.0, 5.0]);
        assert!(rb.is_empty());
    }

    #[test]
    fn test_spsc_ring_buffer_flush_consumer() {
        let rb = SpscRingBuffer::<f32, 128>::new();
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        assert_eq!(rb.push_slice(&data), 8);
        assert_eq!(rb.available_read(), 8);

        let discarded = rb.flush_consumer();
        assert_eq!(discarded, 8);
        assert_eq!(rb.available_read(), 0);
        assert!(rb.is_empty());

        // Further writes and reads work seamlessly without index desync
        let next_data = vec![42.0f32; 16];
        assert_eq!(rb.push_slice(&next_data), 16);
        assert_eq!(rb.available_read(), 16);

        let mut out = vec![0.0f32; 16];
        assert_eq!(rb.pop_slice(&mut out), 16);
        assert_eq!(out, next_data);
    }

    #[test]
    fn test_audio_ring_buffer_f32_dynamic() {
        let rb = AudioRingBufferF32::new(512);
        assert_eq!(rb.capacity(), 512);

        let pcm_chunk: Vec<f32> = (0..160).map(|i| i as f32 * 0.01).collect();
        let written = rb.push_slice(&pcm_chunk);
        assert_eq!(written, 160);
        assert_eq!(rb.available_read(), 160);

        let mut read_buf = vec![0.0f32; 160];
        let read = rb.pop_slice(&mut read_buf);
        assert_eq!(read, 160);
        assert_eq!(read_buf, pcm_chunk);
        assert!(rb.is_empty());
    }

    #[test]
    fn test_audio_ring_buffer_wrap_around() {
        let rb = AudioRingBufferF32::new(64);
        let mut scratch = vec![0.0f32; 20];

        for i in 0..100 {
            let data: Vec<f32> = (0..20).map(|j| (i * 20 + j) as f32).collect();
            let w = rb.push_slice(&data);
            assert_eq!(w, 20);

            let r = rb.pop_slice(&mut scratch);
            assert_eq!(r, 20);
            assert_eq!(scratch, data);
        }
        assert!(rb.is_empty());
    }

    #[test]
    fn test_audio_ring_buffer_peek_and_skip() {
        let rb = AudioRingBufferF32::new(128);
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        rb.push_slice(&data);

        let mut peek_buf = vec![0.0f32; 5];
        let peeked = rb.peek_slice(&mut peek_buf);
        assert_eq!(peeked, 5);
        assert_eq!(peek_buf, data);
        assert_eq!(rb.available_read(), 5, "Peek must not consume samples");

        let skipped = rb.skip(2);
        assert_eq!(skipped, 2);
        assert_eq!(rb.available_read(), 3);

        let mut rest = vec![0.0f32; 3];
        rb.pop_slice(&mut rest);
        assert_eq!(rest, vec![3.0, 4.0, 5.0]);
    }

    #[test]
    fn test_audio_ring_buffer_flush_consumer() {
        let rb = AudioRingBufferF32::new(128);
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        rb.push_slice(&data);
        assert_eq!(rb.available_read(), 8);

        let discarded = rb.flush_consumer();
        assert_eq!(discarded, 8);
        assert_eq!(rb.available_read(), 0);
        assert!(rb.is_empty());

        let (_underruns, _overruns, total_w, total_r) = rb.metrics();
        assert_eq!(total_w, 8);
        assert_eq!(total_r, 8);

        // Further writes and reads maintain metrics and FIFO integrity
        let next_data = vec![99.0f32; 32];
        rb.push_slice(&next_data);
        assert_eq!(rb.available_read(), 32);

        let mut out = vec![0.0f32; 32];
        let r = rb.pop_slice(&mut out);
        assert_eq!(r, 32);
        assert_eq!(out, next_data);

        let (_, _, total_w2, total_r2) = rb.metrics();
        assert_eq!(total_w2, 40);
        assert_eq!(total_r2, 40);
    }

    #[test]
    fn test_audio_ring_buffer_overrun_and_underrun_metrics() {
        let rb = AudioRingBufferF32::new(64);
        let data = vec![1.0f32; 80];
        let written = rb.push_slice(&data);
        assert_eq!(written, 64);
        assert!(rb.is_full());

        let mut dst = vec![0.0f32; 100];
        let read = rb.pop_slice(&mut dst);
        assert_eq!(read, 64);
        assert!(rb.is_empty());

        let (underruns, overruns, total_w, total_r) = rb.metrics();
        assert_eq!(overruns, 1);
        assert_eq!(underruns, 1);
        assert_eq!(total_w, 64);
        assert_eq!(total_r, 64);
    }

    #[test]
    fn test_audio_ring_buffer_invalid_capacity() {
        assert!(AudioRingBufferF32::try_new(0).is_err());
        assert!(AudioRingBufferF32::try_new(63).is_err());
        assert!(AudioRingBufferF32::try_new(100).is_err());
        assert!(AudioRingBufferF32::try_new(64).is_ok());
    }

    #[test]
    fn test_duplex_ring_buffer_full_cycle() {
        let duplex = DuplexAudioRingBuffer::new(16000, 24000);
        assert_eq!(duplex.sample_rate_capture(), 16000);
        assert_eq!(duplex.sample_rate_playback(), 24000);

        let mic_in = vec![0.1f32; 160];
        let spk_in = vec![0.8f32; 240];

        duplex.push_capture(&mic_in);
        duplex.push_playback(&spk_in);

        let mut mic_out = vec![0.0f32; 160];
        let mut spk_out = vec![0.0f32; 240];

        assert_eq!(duplex.pop_capture(&mut mic_out), 160);
        assert_eq!(duplex.pop_playback(&mut spk_out), 240);

        assert_eq!(mic_out, mic_in);
        assert_eq!(spk_out, spk_in);

        // Test barge-in flush
        duplex.push_playback(&spk_in);
        duplex.flush_playback();
        assert_eq!(duplex.playback_ring.available_read(), 0);
    }

    #[test]
    fn test_pcm_format_conversions() {
        let f32_samples = vec![-1.0f32, -0.5, 0.0, 0.5, 1.0, 1.5, -2.0];
        let mut i16_samples = vec![0i16; f32_samples.len()];

        f32_to_i16_slice(&f32_samples, &mut i16_samples);
        assert_eq!(i16_samples[0], -32768);
        assert_eq!(i16_samples[2], 0);
        assert_eq!(i16_samples[4], 32767);
        assert_eq!(i16_samples[5], 32767, "Must clamp positive overload");
        assert_eq!(i16_samples[6], -32768, "Must clamp negative overload");

        let mut roundtrip_f32 = vec![0.0f32; 5];
        i16_to_f32_slice(&i16_samples[..5], &mut roundtrip_f32);
        assert!((roundtrip_f32[0] - (-1.0)).abs() < 1e-4);
        assert!((roundtrip_f32[2] - 0.0).abs() < 1e-4);
        assert!((roundtrip_f32[4] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn test_linear_resampling() {
        let src_16k: Vec<f32> = (0..160).map(|i| (i as f32 * 0.1).sin()).collect();
        let mut dst_24k = Vec::new();
        resample_linear_16k_to_24k(&src_16k, &mut dst_24k);
        assert_eq!(dst_24k.len(), 240);
        assert!(dst_24k.iter().all(|s| s.is_finite()));

        let mut dst_16k = Vec::new();
        resample_linear_24k_to_16k(&dst_24k, &mut dst_16k);
        assert_eq!(dst_16k.len(), 160);
        assert!(dst_16k.iter().all(|s| s.is_finite()));

        // Test empty
        let empty: Vec<f32> = Vec::new();
        let mut out = Vec::new();
        resample_linear_16k_to_24k(&empty, &mut out);
        assert!(out.is_empty());
        resample_linear_24k_to_16k(&empty, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn test_concurrent_spsc_audio_throughput() {
        let rb = Arc::new(AudioRingBufferF32::new(16384));
        let num_samples = 200_000;
        let chunk_size = 160;

        let rb_prod = Arc::clone(&rb);
        let prod = thread::spawn(move || {
            let chunk = vec![0.42f32; chunk_size];
            let mut sent = 0;
            while sent < num_samples {
                let to_send = chunk_size.min(num_samples - sent);
                let w = rb_prod.push_slice(&chunk[..to_send]);
                sent += w;
                if w == 0 {
                    std::hint::spin_loop();
                }
            }
        });

        let rb_cons = Arc::clone(&rb);
        let cons = thread::spawn(move || {
            let mut dst = vec![0.0f32; chunk_size];
            let mut received = 0;
            while received < num_samples {
                let to_read = chunk_size.min(num_samples - received);
                let r = rb_cons.pop_slice(&mut dst[..to_read]);
                received += r;
                if r == 0 {
                    std::hint::spin_loop();
                }
            }
            received
        });

        prod.join().unwrap();
        let total = cons.join().unwrap();
        assert_eq!(total, num_samples);
        assert!(rb.is_empty());
    }

    #[test]
    fn test_concurrent_const_generic_spsc_throughput() {
        let rb = Arc::new(SpscRingBuffer::<f32, 16384>::new());
        let num_samples = 100_000;
        let chunk_size = 160;

        let rb_prod = Arc::clone(&rb);
        let prod = thread::spawn(move || {
            let chunk = vec![0.88f32; chunk_size];
            let mut sent = 0;
            while sent < num_samples {
                let to_send = chunk_size.min(num_samples - sent);
                let w = rb_prod.push_slice(&chunk[..to_send]);
                sent += w;
                if w == 0 {
                    std::hint::spin_loop();
                }
            }
        });

        let rb_cons = Arc::clone(&rb);
        let cons = thread::spawn(move || {
            let mut dst = vec![0.0f32; chunk_size];
            let mut received = 0;
            while received < num_samples {
                let to_read = chunk_size.min(num_samples - received);
                let r = rb_cons.pop_slice(&mut dst[..to_read]);
                received += r;
                if r == 0 {
                    std::hint::spin_loop();
                }
            }
            received
        });

        prod.join().unwrap();
        let total = cons.join().unwrap();
        assert_eq!(total, num_samples);
        assert!(rb.is_empty());
    }

    #[test]
    fn test_concurrent_flush_and_pop_stress() {
        let rb = Arc::new(AudioRingBufferF32::new(8192));
        let num_samples = 300_000;
        let chunk_size = 160;
        let running = Arc::new(AtomicBool::new(true));

        let rb_prod = Arc::clone(&rb);
        let prod = thread::spawn(move || {
            let chunk = vec![0.5f32; chunk_size];
            let mut sent = 0;
            while sent < num_samples {
                let to_send = chunk_size.min(num_samples - sent);
                let w = rb_prod.push_slice(&chunk[..to_send]);
                sent += w;
                if w == 0 {
                    std::hint::spin_loop();
                }
            }
        });

        let rb_cons = Arc::clone(&rb);
        let running_cons = Arc::clone(&running);
        let cons = thread::spawn(move || {
            let mut dst = vec![0.0f32; chunk_size];
            let mut read_total = 0;
            while running_cons.load(Ordering::Relaxed) || rb_cons.available_read() > 0 {
                let r = rb_cons.pop_slice(&mut dst);
                read_total += r;
                if r == 0 {
                    std::hint::spin_loop();
                }
            }
            read_total
        });

        // 2 Concurrent Barge-In threads calling flush_consumer() repeatedly
        let mut flush_handles = Vec::new();
        for _ in 0..2 {
            let rb_flush = Arc::clone(&rb);
            let running_flush = Arc::clone(&running);
            flush_handles.push(thread::spawn(move || {
                let mut flushes = 0;
                while running_flush.load(Ordering::Relaxed) {
                    let discarded = rb_flush.flush_consumer();
                    if discarded > 0 {
                        flushes += 1;
                    }
                    std::thread::yield_now();
                }
                flushes
            }));
        }

        prod.join().unwrap();
        running.store(false, Ordering::Relaxed);

        let read_samples = cons.join().unwrap();
        for h in flush_handles {
            h.join().unwrap();
        }

        let (_underruns, _overruns, total_w, total_r) = rb.metrics();
        assert_eq!(total_w, num_samples as u64, "All written samples must be accounted for");
        assert_eq!(
            total_r + rb.available_read() as u64,
            total_w,
            "Total read + remaining unread must equal total written"
        );
        assert!(read_samples <= num_samples, "Read samples cannot exceed total written");
    }

    #[test]
    fn test_spsc_ring_buffer_concurrent_flush_cas_integrity() {
        let rb = Arc::new(SpscRingBuffer::<f32, 1024>::new());
        let num_samples = 150_000;
        let chunk_size = 128;
        let running = Arc::new(AtomicBool::new(true));

        let rb_prod = Arc::clone(&rb);
        let prod = thread::spawn(move || {
            let chunk = vec![0.77f32; chunk_size];
            let mut sent = 0;
            while sent < num_samples {
                let to_send = chunk_size.min(num_samples - sent);
                let w = rb_prod.push_slice(&chunk[..to_send]);
                sent += w;
                if w == 0 {
                    std::hint::spin_loop();
                }
            }
        });

        let rb_cons = Arc::clone(&rb);
        let running_cons = Arc::clone(&running);
        let cons = thread::spawn(move || {
            let mut dst = vec![0.0f32; chunk_size];
            let mut read_total = 0;
            while running_cons.load(Ordering::Relaxed) || rb_cons.available_read() > 0 {
                let r = rb_cons.pop_slice(&mut dst);
                read_total += r;
                if r == 0 {
                    std::hint::spin_loop();
                }
            }
            read_total
        });

        let mut flush_handles = Vec::new();
        for _ in 0..2 {
            let rb_flush = Arc::clone(&rb);
            let running_flush = Arc::clone(&running);
            flush_handles.push(thread::spawn(move || {
                let mut flushes = 0;
                while running_flush.load(Ordering::Relaxed) {
                    let discarded = rb_flush.flush_consumer();
                    if discarded > 0 {
                        flushes += 1;
                    }
                    std::thread::yield_now();
                }
                flushes
            }));
        }

        prod.join().unwrap();
        running.store(false, Ordering::Relaxed);

        let _ = cons.join().unwrap();
        for h in flush_handles {
            h.join().unwrap();
        }

        assert!(rb.available_read() <= 1024, "Available read must not exceed capacity");
    }

    #[test]
    fn test_barge_in_preemption_sample_purity() {
        let duplex = DuplexAudioRingBuffer::new(16000, 16000);

        // Producer writes Turn 1 audio samples (signature value 1.0)
        let turn1_samples = vec![1.0f32; 1600];
        let written1 = duplex.push_playback(&turn1_samples);
        assert_eq!(written1, 1600);

        // Consumer reads 320 samples from Turn 1
        let mut read_buf = vec![0.0f32; 320];
        let read1 = duplex.pop_playback(&mut read_buf);
        assert_eq!(read1, 320);
        assert!(read_buf.iter().all(|&s| (s - 1.0).abs() < f32::EPSILON));

        // Barge-In occurs: user interrupts, audio output is flushed
        let discarded = duplex.flush_playback();
        assert_eq!(discarded, 1600 - 320);
        assert_eq!(duplex.playback_ring.available_read(), 0);

        // Producer immediately writes Turn 2 audio samples (signature value 2.0)
        let turn2_samples = vec![2.0f32; 1600];
        let written2 = duplex.push_playback(&turn2_samples);
        assert_eq!(written2, 1600);

        // Consumer reads all remaining samples
        let mut all_read_turn2 = Vec::new();
        let mut chunk = vec![0.0f32; 320];
        while duplex.playback_ring.available_read() > 0 {
            let r = duplex.pop_playback(&mut chunk);
            if r > 0 {
                all_read_turn2.extend_from_slice(&chunk[..r]);
            }
        }

        assert_eq!(all_read_turn2.len(), 1600);
        // CRITICAL PURITY INVARIANT: No Turn 1 sample (1.0) must leak into post-flush reads
        for (i, &sample) in all_read_turn2.iter().enumerate() {
            assert_eq!(
                sample, 2.0f32,
                "Sample at index {} corrupted: expected 2.0 (Turn 2), found {} (possible stale Turn 1 leak)",
                i, sample
            );
        }
    }

    #[test]
    fn test_spsc_ring_buffer_barge_in_preemption_purity() {
        let rb = SpscRingBuffer::<f32, 2048>::new();

        // Producer writes Turn 1 audio
        let turn1 = vec![1.0f32; 1000];
        assert_eq!(rb.push_slice(&turn1), 1000);

        // Consumer reads 200 samples
        let mut buf = vec![0.0f32; 200];
        assert_eq!(rb.pop_slice(&mut buf), 200);
        assert!(buf.iter().all(|&s| (s - 1.0).abs() < f32::EPSILON));

        // Flush on Barge-In
        let discarded = rb.flush_consumer();
        assert_eq!(discarded, 800);
        assert_eq!(rb.available_read(), 0);

        // Producer writes Turn 2 audio
        let turn2 = vec![2.0f32; 1000];
        assert_eq!(rb.push_slice(&turn2), 1000);

        // Consumer reads all Turn 2 audio
        let mut turn2_read = Vec::new();
        let mut chunk = vec![0.0f32; 100];
        while rb.available_read() > 0 {
            let r = rb.pop_slice(&mut chunk);
            if r > 0 {
                turn2_read.extend_from_slice(&chunk[..r]);
            }
        }

        assert_eq!(turn2_read.len(), 1000);
        for (idx, &s) in turn2_read.iter().enumerate() {
            assert_eq!(s, 2.0f32, "Stale sample at {} in SpscRingBuffer", idx);
        }
    }
}
