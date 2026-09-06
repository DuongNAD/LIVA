//! Lock-free Single-Producer Single-Consumer (SPSC) Ring Buffer with 64-byte Cache-Line Alignment.
//!
//! Designed for high-throughput, sub-microsecond inter-process and intra-process communication
//! between `liva-native-core` and frontend/companion subsystems (RFC-003 Milestone 3).

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::codec::{
    calculate_checksum, FrameHeader, FrameType, IpcError, IpcFrameRef, ZeroCopyDeserializable,
    ZeroCopySerializable, FRAME_MAGIC, FRAME_VERSION_1, MAX_PAYLOAD_SIZE,
};

/// CPU Cache-line size in bytes on modern x86_64 and ARM64 architectures.
pub const CACHE_LINE_BYTES: usize = 64;

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

/// Single-Producer Single-Consumer Lock-Free Circular Ring Buffer.
///
/// Guarantees:
/// - Cache-line aligned `head` and `tail` pointers (64-byte alignment) to avoid false sharing.
/// - Atomic Acquire/Release synchronization for thread-safe lock-free operation.
/// - Power-of-two capacity for branchless modulo operations (`index & mask`).
/// - Strict bounds checking against `MAX_PAYLOAD_SIZE` (64MB) to prevent integer overflow and OOM attacks.
/// - RAII `Drop` implementation safely deallocating aligned buffer memory.
pub struct SpscRingBuffer {
    buffer_ptr: *mut u8,
    capacity: usize,
    mask: usize,
    head: CacheAlignedAtomic, // Producer index
    tail: CacheAlignedAtomic, // Consumer index
}

unsafe impl Send for SpscRingBuffer {}
unsafe impl Sync for SpscRingBuffer {}

impl SpscRingBuffer {
    /// Creates a new `SpscRingBuffer` with the specified power-of-two capacity.
    ///
    /// # Panics
    /// Panics if `capacity` is not a power of two or allocation fails.
    pub fn new(capacity: usize) -> Self {
        Self::try_new(capacity).expect("Failed to initialize SpscRingBuffer")
    }

    /// Tries to create a new `SpscRingBuffer` with the specified capacity.
    pub fn try_new(capacity: usize) -> Result<Self, IpcError> {
        if capacity == 0 || !capacity.is_power_of_two() {
            return Err(IpcError::Validation(format!(
                "Ring buffer capacity must be a non-zero power of 2, got {}",
                capacity
            )));
        }
        if capacity < CACHE_LINE_BYTES {
            return Err(IpcError::Validation(format!(
                "Ring buffer capacity must be at least {} bytes, got {}",
                CACHE_LINE_BYTES, capacity
            )));
        }

        let layout = Layout::from_size_align(capacity, CACHE_LINE_BYTES)
            .map_err(|e| IpcError::Validation(format!("Invalid memory layout: {}", e)))?;

        let buffer_ptr = unsafe { alloc_zeroed(layout) };
        if buffer_ptr.is_null() {
            return Err(IpcError::Validation(
                "Memory allocation failed for SpscRingBuffer".into(),
            ));
        }

        Ok(Self {
            buffer_ptr,
            capacity,
            mask: capacity - 1,
            head: CacheAlignedAtomic::new(0),
            tail: CacheAlignedAtomic::new(0),
        })
    }

    /// Returns the total capacity of the ring buffer in bytes.
    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the number of bytes currently occupied in the ring buffer.
    #[inline(always)]
    pub fn occupied_bytes(&self) -> usize {
        let head = self.head.value.load(Ordering::Acquire);
        let tail = self.tail.value.load(Ordering::Acquire);
        head.wrapping_sub(tail)
    }

    /// Returns the number of available bytes remaining for writing.
    #[inline(always)]
    pub fn available_write_space(&self) -> usize {
        let head = self.head.value.load(Ordering::Relaxed);
        let tail = self.tail.value.load(Ordering::Acquire);
        self.capacity.saturating_sub(head.wrapping_sub(tail))
    }

    /// Returns true if the ring buffer is completely empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        let head = self.head.value.load(Ordering::Acquire);
        let tail = self.tail.value.load(Ordering::Acquire);
        head == tail
    }

    /// Writes raw payload bytes as a length-prefixed frame.
    pub fn write_slice(&self, bytes: &[u8]) -> Result<(), IpcError> {
        let payload_len = bytes.len();
        if payload_len == 0 {
            return Err(IpcError::Validation("Payload length cannot be 0".into()));
        }
        if payload_len > MAX_PAYLOAD_SIZE {
            return Err(IpcError::PayloadTooLarge {
                size: payload_len,
                max: MAX_PAYLOAD_SIZE,
            });
        }
        if payload_len > self.capacity.saturating_sub(std::mem::size_of::<u32>()) {
            return Err(IpcError::PayloadTooLarge {
                size: payload_len,
                max: self.capacity.saturating_sub(std::mem::size_of::<u32>()),
            });
        }

        let head = self.head.value.load(Ordering::Relaxed);
        let tail = self.tail.value.load(Ordering::Acquire);

        let available = self.capacity.saturating_sub(head.wrapping_sub(tail));
        let total_required = payload_len + std::mem::size_of::<u32>();

        if available < total_required {
            return Err(IpcError::RingBufferFull);
        }

        unsafe {
            // 1. Write 4-byte little-endian length prefix
            let write_offset = head & self.mask;
            let len_bytes = (payload_len as u32).to_le_bytes();
            self.copy_bytes_wrapped(write_offset, &len_bytes);

            // 2. Write payload bytes
            let data_offset = head.wrapping_add(std::mem::size_of::<u32>()) & self.mask;
            self.copy_bytes_wrapped(data_offset, bytes);
        }

        // Store new head with Release barrier so consumer sees written data
        self.head
            .value
            .store(head.wrapping_add(total_required), Ordering::Release);
        Ok(())
    }

    /// Reads raw payload bytes from the ring buffer into the provided scratch buffer.
    pub fn read_bytes(&self, scratch_buf: &mut Vec<u8>) -> Result<Option<usize>, IpcError> {
        let tail = self.tail.value.load(Ordering::Relaxed);
        let head = self.head.value.load(Ordering::Acquire);

        let used = head.wrapping_sub(tail);
        if used < std::mem::size_of::<u32>() {
            return Ok(None);
        }

        let read_offset = tail & self.mask;
        let mut len_bytes = [0u8; 4];
        unsafe {
            self.read_bytes_wrapped(read_offset, &mut len_bytes);
        }
        let payload_len = u32::from_le_bytes(len_bytes) as usize;

        // Header corruption and bounds validation
        if payload_len == 0 {
            return Err(IpcError::Validation(
                "Corrupted IPC frame header: length is 0".into(),
            ));
        }
        if payload_len > MAX_PAYLOAD_SIZE {
            return Err(IpcError::PayloadTooLarge {
                size: payload_len,
                max: MAX_PAYLOAD_SIZE,
            });
        }
        let max_possible = self.capacity.saturating_sub(std::mem::size_of::<u32>());
        if payload_len > max_possible {
            return Err(IpcError::Validation(format!(
                "Corrupted IPC frame header: length {} exceeds buffer capacity {}",
                payload_len, self.capacity
            )));
        }

        let total_consumed = payload_len + std::mem::size_of::<u32>();
        if used < total_consumed {
            return Err(IpcError::Validation(format!(
                "Incomplete IPC frame: used bytes ({}) < required ({})",
                used, total_consumed
            )));
        }

        scratch_buf.resize(payload_len, 0);
        let data_offset = tail.wrapping_add(std::mem::size_of::<u32>()) & self.mask;
        unsafe {
            self.read_bytes_wrapped(data_offset, scratch_buf);
        }

        // Advance tail with Release barrier
        self.tail
            .value
            .store(tail.wrapping_add(total_consumed), Ordering::Release);

        Ok(Some(payload_len))
    }

    /// Writes a structured IPC frame with typed header and checksum.
    pub fn write_ipc_frame(&self, frame_type: FrameType, payload: &[u8]) -> Result<(), IpcError> {
        let payload_len = payload.len();
        if payload_len > MAX_PAYLOAD_SIZE {
            return Err(IpcError::PayloadTooLarge {
                size: payload_len,
                max: MAX_PAYLOAD_SIZE,
            });
        }

        let checksum = calculate_checksum(payload);
        let header = FrameHeader {
            magic: FRAME_MAGIC,
            version: FRAME_VERSION_1,
            frame_type: frame_type as u16,
            flags: 0,
            payload_len: payload_len as u32,
            checksum,
            _reserved: 0,
        };

        let header_bytes = bytemuck::bytes_of(&header);
        let total_frame_len = header_bytes.len() + payload_len;

        let head = self.head.value.load(Ordering::Relaxed);
        let tail = self.tail.value.load(Ordering::Acquire);

        let available = self.capacity.saturating_sub(head.wrapping_sub(tail));
        let total_required = total_frame_len + std::mem::size_of::<u32>();

        if available < total_required {
            return Err(IpcError::RingBufferFull);
        }

        unsafe {
            // Write length prefix (total_frame_len)
            let write_offset = head & self.mask;
            let len_bytes = (total_frame_len as u32).to_le_bytes();
            self.copy_bytes_wrapped(write_offset, &len_bytes);

            // Write FrameHeader
            let header_offset = head.wrapping_add(std::mem::size_of::<u32>()) & self.mask;
            self.copy_bytes_wrapped(header_offset, header_bytes);

            // Write Payload
            let payload_offset = head.wrapping_add(std::mem::size_of::<u32>() + header_bytes.len())
                & self.mask;
            self.copy_bytes_wrapped(payload_offset, payload);
        }

        self.head
            .value
            .store(head.wrapping_add(total_required), Ordering::Release);
        Ok(())
    }

    /// Reads an IPC frame into `scratch_buf` and validates its header and checksum.
    pub fn read_ipc_frame<'a>(
        &self,
        scratch_buf: &'a mut Vec<u8>,
    ) -> Result<Option<IpcFrameRef<'a>>, IpcError> {
        let read_len = match self.read_bytes(scratch_buf)? {
            Some(len) => len,
            None => return Ok(None),
        };

        if read_len < std::mem::size_of::<FrameHeader>() {
            return Err(IpcError::FrameSizeMismatch {
                expected: std::mem::size_of::<FrameHeader>(),
                actual: read_len,
            });
        }

        let (header_slice, payload_slice) = scratch_buf.split_at(std::mem::size_of::<FrameHeader>());
        let header: &FrameHeader = bytemuck::try_from_bytes(header_slice)
            .map_err(|_| IpcError::Validation("Corrupted FrameHeader alignment or layout".into()))?;

        if header.magic != FRAME_MAGIC {
            return Err(IpcError::Validation(format!(
                "Invalid frame magic: {:?}, expected {:?}",
                header.magic, FRAME_MAGIC
            )));
        }
        if header.version != FRAME_VERSION_1 {
            return Err(IpcError::Validation(format!(
                "Unsupported frame version: {}, expected {}",
                header.version, FRAME_VERSION_1
            )));
        }

        let payload_len = header.payload_len as usize;
        if payload_slice.len() != payload_len {
            return Err(IpcError::FrameSizeMismatch {
                expected: payload_len,
                actual: payload_slice.len(),
            });
        }

        let computed_checksum = calculate_checksum(payload_slice);
        if header.checksum != computed_checksum {
            return Err(IpcError::ChecksumMismatch {
                expected: header.checksum,
                calculated: computed_checksum,
            });
        }

        Ok(Some(IpcFrameRef {
            header,
            payload: payload_slice,
        }))
    }

    /// Writes a zero-copy serializable frame into the ring buffer.
    pub fn write_frame<T: ZeroCopySerializable>(&self, frame: &T) -> Result<(), IpcError> {
        let encoded_len = frame.encoded_len();
        if encoded_len == 0 {
            return Err(IpcError::Validation("Frame encoded length cannot be 0".into()));
        }
        if encoded_len > MAX_PAYLOAD_SIZE {
            return Err(IpcError::PayloadTooLarge {
                size: encoded_len,
                max: MAX_PAYLOAD_SIZE,
            });
        }

        let mut encoded = vec![0u8; encoded_len];
        frame.encode_to_slice(&mut encoded)?;
        self.write_ipc_frame(frame.frame_type(), &encoded)
    }

    /// Reads a zero-copy deserializable frame from the ring buffer.
    pub fn read_frame<'a, T: ZeroCopyDeserializable<'a>>(
        &self,
        scratch_buf: &'a mut Vec<u8>,
    ) -> Result<Option<T>, IpcError> {
        let frame_ref = match self.read_ipc_frame(scratch_buf)? {
            Some(f) => f,
            None => return Ok(None),
        };
        let decoded = T::decode_from_slice(frame_ref.payload)?;
        Ok(Some(decoded))
    }

    /// Helper alias for zero-copy write serialization (contract compatibility).
    #[inline(always)]
    pub fn write_archived<T: ZeroCopySerializable>(&self, value: &T) -> Result<(), IpcError> {
        self.write_frame(value)
    }

    /// Helper alias for zero-copy read deserialization (contract compatibility).
    #[inline(always)]
    pub fn read_archived<'a, T: ZeroCopyDeserializable<'a>>(
        &self,
        scratch_buf: &'a mut Vec<u8>,
    ) -> Result<Option<T>, IpcError> {
        self.read_frame::<T>(scratch_buf)
    }

    /// Resets the ring buffer head and tail to zero.
    ///
    /// # Safety
    /// Caller must ensure no concurrent reads or writes are occurring.
    pub fn clear(&self) {
        self.head.value.store(0, Ordering::Release);
        self.tail.value.store(0, Ordering::Release);
    }

    #[inline(always)]
    unsafe fn copy_bytes_wrapped(&self, offset: usize, src: &[u8]) {
        let len = src.len();
        if len == 0 {
            return;
        }
        let first_chunk = (self.capacity - offset).min(len);
        unsafe {
            std::ptr::copy_nonoverlapping(src.as_ptr(), self.buffer_ptr.add(offset), first_chunk);
            if first_chunk < len {
                let remainder = len - first_chunk;
                std::ptr::copy_nonoverlapping(
                    src.as_ptr().add(first_chunk),
                    self.buffer_ptr,
                    remainder,
                );
            }
        }
    }

    #[inline(always)]
    unsafe fn read_bytes_wrapped(&self, offset: usize, dst: &mut [u8]) {
        let len = dst.len();
        if len == 0 {
            return;
        }
        let first_chunk = (self.capacity - offset).min(len);
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.buffer_ptr.add(offset),
                dst.as_mut_ptr(),
                first_chunk,
            );
            if first_chunk < len {
                let remainder = len - first_chunk;
                std::ptr::copy_nonoverlapping(
                    self.buffer_ptr,
                    dst.as_mut_ptr().add(first_chunk),
                    remainder,
                );
            }
        }
    }
}

impl Drop for SpscRingBuffer {
    fn drop(&mut self) {
        if !self.buffer_ptr.is_null() && self.capacity > 0 {
            let layout = Layout::from_size_align(self.capacity, CACHE_LINE_BYTES)
                .expect("Failed to create layout for SpscRingBuffer deallocation");
            unsafe {
                dealloc(self.buffer_ptr, layout);
            }
            self.buffer_ptr = std::ptr::null_mut();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::codec::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_cache_line_alignment() {
        assert_eq!(std::mem::align_of::<CacheAlignedAtomic>(), 64);
        assert_eq!(std::mem::size_of::<CacheAlignedAtomic>(), 64);

        let rb = SpscRingBuffer::new(1024);
        let head_addr = &rb.head as *const _ as usize;
        let tail_addr = &rb.tail as *const _ as usize;
        assert_eq!(head_addr % 64, 0, "Head must be 64-byte aligned");
        assert_eq!(tail_addr % 64, 0, "Tail must be 64-byte aligned");
        assert!(tail_addr.abs_diff(head_addr) >= 64, "Head and Tail must not share cache line");
    }

    #[test]
    fn test_basic_read_write() {
        let rb = SpscRingBuffer::new(1024);
        assert!(rb.is_empty());

        let payload = b"Hello LIVA Zero-Copy IPC!";
        rb.write_slice(payload).expect("Write should succeed");
        assert!(!rb.is_empty());

        let mut scratch = Vec::new();
        let read_len = rb.read_bytes(&mut scratch).expect("Read should succeed");
        assert_eq!(read_len, Some(payload.len()));
        assert_eq!(&scratch[..], payload);
        assert!(rb.is_empty());
    }

    #[test]
    fn test_wrap_around_handling() {
        // Buffer capacity = 64 bytes
        let rb = SpscRingBuffer::new(64);
        let mut scratch = Vec::new();

        // Write and read repeatedly to force wrap-around
        for i in 0..100 {
            let msg = format!("chunk-{:04}", i);
            rb.write_slice(msg.as_bytes()).expect("Write chunk");
            let read_len = rb.read_bytes(&mut scratch).expect("Read chunk");
            assert_eq!(read_len, Some(msg.len()));
            assert_eq!(std::str::from_utf8(&scratch).unwrap(), msg);
        }
        assert!(rb.is_empty());
    }

    #[test]
    fn test_buffer_full_rejection() {
        let rb = SpscRingBuffer::new(128);
        let chunk = vec![0xAB; 60]; // 60 + 4 = 64 bytes
        rb.write_slice(&chunk).expect("First write");
        rb.write_slice(&chunk).expect("Second write"); // 128 bytes total

        // Third write should fail because available space is 0
        let err = rb.write_slice(&chunk).unwrap_err();
        assert_eq!(err, IpcError::RingBufferFull);
    }

    #[test]
    fn test_corrupted_header_rejection() {
        let rb = SpscRingBuffer::new(1024);

        // 1. Zero-length payload
        let err = rb.write_slice(&[]).unwrap_err();
        assert!(matches!(err, IpcError::Validation(_)));

        // 2. Corrupted giant length header simulation (e.g. 0xFFFFFFFF)
        unsafe {
            let giant_len: u32 = 0xFFFFFFFF;
            rb.copy_bytes_wrapped(0, &giant_len.to_le_bytes());
            rb.head.value.store(100, Ordering::Release);
        }

        let mut scratch = Vec::new();
        let err = rb.read_bytes(&mut scratch).unwrap_err();
        assert!(matches!(err, IpcError::PayloadTooLarge { .. }));
    }

    #[test]
    fn test_spsc_concurrent_high_throughput() {
        let capacity = 64 * 1024; // 64KB
        let rb = Arc::new(SpscRingBuffer::new(capacity));
        let num_messages = 50_000;

        let rb_prod = Arc::clone(&rb);
        let prod_handle = thread::spawn(move || {
            for i in 0..num_messages {
                let msg = (i as u64).to_le_bytes();
                while let Err(IpcError::RingBufferFull) = rb_prod.write_slice(&msg) {
                    std::hint::spin_loop();
                }
            }
        });

        let rb_cons = Arc::clone(&rb);
        let cons_handle = thread::spawn(move || {
            let mut scratch = Vec::new();
            let mut received = 0;
            while received < num_messages {
                match rb_cons.read_bytes(&mut scratch) {
                    Ok(Some(len)) => {
                        assert_eq!(len, 8);
                        let val = u64::from_le_bytes(scratch[..8].try_into().unwrap());
                        assert_eq!(val, received as u64);
                        received += 1;
                    }
                    Ok(None) => {
                        std::hint::spin_loop();
                    }
                    Err(e) => panic!("Read error: {:?}", e),
                }
            }
            received
        });

        prod_handle.join().unwrap();
        let total_received = cons_handle.join().unwrap();
        assert_eq!(total_received, num_messages);
        assert!(rb.is_empty());
    }

    #[test]
    fn test_empty_buffer_read() {
        let rb = SpscRingBuffer::new(1024);
        let mut scratch = Vec::new();
        let res = rb.read_bytes(&mut scratch).expect("Read empty should succeed");
        assert_eq!(res, None);

        let res_ipc = rb.read_ipc_frame(&mut scratch).expect("Read IPC empty should succeed");
        assert!(res_ipc.is_none());
    }

    #[test]
    fn test_ring_buffer_clear() {
        let rb = SpscRingBuffer::new(1024);
        rb.write_slice(b"temporary message").unwrap();
        assert!(!rb.is_empty());
        assert!(rb.occupied_bytes() > 0);

        rb.clear();
        assert!(rb.is_empty());
        assert_eq!(rb.occupied_bytes(), 0);

        // Can write again after clear
        rb.write_slice(b"fresh message").unwrap();
        let mut scratch = Vec::new();
        let res = rb.read_bytes(&mut scratch).unwrap();
        assert_eq!(res, Some(13));
        assert_eq!(&scratch[..], b"fresh message");
    }

    #[test]
    fn test_payload_too_large_rejection() {
        let rb = SpscRingBuffer::new(1024);
        // Attempt to write payload larger than capacity
        let big_data = vec![0u8; 2048];
        let err = rb.write_slice(&big_data).unwrap_err();
        assert!(matches!(err, IpcError::PayloadTooLarge { .. }));

        // Attempt to write frame exceeding MAX_PAYLOAD_SIZE
        let err2 = rb
            .write_ipc_frame(FrameType::Custom, &vec![0u8; MAX_PAYLOAD_SIZE + 1])
            .unwrap_err();
        assert!(matches!(err2, IpcError::PayloadTooLarge { .. }));
    }

    #[test]
    fn test_structured_frames_through_ring_buffer() {
        let rb = SpscRingBuffer::new(64 * 1024);

        // 1. Write TelemetryFrame
        let telemetry = TelemetryFrame {
            timestamp_ns: 123456789,
            ttft_ns: 15_000_000,
            total_duration_ns: 500_000_000,
            tokens_generated: 25,
            prompt_tokens: 100,
            db_read_latency_ns: 200_000,
            db_write_latency_ns: 300_000,
            memory_rss_bytes: 30 * 1024 * 1024,
            cpu_usage_percent: 5.5,
            voice_queue_depth: 0,
            user_queue_depth: 0,
            bg_queue_depth: 1,
            preemption_count: 0,
            _reserved: 0,
        };
        rb.write_frame(&telemetry).expect("Write telemetry frame");

        // 2. Write TokenDeltaFrame
        let token_delta = TokenDeltaFrame {
            task_id: uuid::Uuid::new_v4(),
            token_id: 10,
            is_first: false,
            is_final: true,
            cumulative_tokens: 25,
            latency_from_start_ns: 450_000_000,
            text: " Hoàn thành phản hồi!".to_string(),
        };
        rb.write_frame(&token_delta).expect("Write token delta frame");

        // 3. Read back TelemetryFrame
        let mut scratch = Vec::new();
        let read_telemetry: &TelemetryFrame = rb
            .read_frame::<&TelemetryFrame>(&mut scratch)
            .expect("Read telemetry frame")
            .expect("Telemetry frame present");
        assert_eq!(read_telemetry.ttft_ns, 15_000_000);
        assert_eq!(read_telemetry.tokens_generated, 25);

        // 4. Read back TokenDeltaFrame
        let read_token: TokenDeltaFrameRef = rb
            .read_frame::<TokenDeltaFrameRef>(&mut scratch)
            .expect("Read token delta frame")
            .expect("Token delta frame present");
        assert_eq!(read_token.header.token_id, 10);
        assert_eq!(read_token.header.is_final, 1);
        assert_eq!(read_token.text, " Hoàn thành phản hồi!");
    }

    #[test]
    fn test_multi_type_concurrent_spsc() {
        let rb = Arc::new(SpscRingBuffer::new(256 * 1024)); // 256KB
        let count = 10_000;

        let rb_prod = Arc::clone(&rb);
        let prod = thread::spawn(move || {
            for i in 0..count {
                let frame = TelemetryFrame {
                    timestamp_ns: i as u64,
                    ttft_ns: (i * 100) as u64,
                    total_duration_ns: (i * 1000) as u64,
                    tokens_generated: i as u64,
                    prompt_tokens: 50,
                    db_read_latency_ns: 1000,
                    db_write_latency_ns: 2000,
                    memory_rss_bytes: 35 * 1024 * 1024,
                    cpu_usage_percent: 10.0,
                    voice_queue_depth: 0,
                    user_queue_depth: 0,
                    bg_queue_depth: 0,
                    preemption_count: 0,
                    _reserved: 0,
                };
                while let Err(IpcError::RingBufferFull) = rb_prod.write_frame(&frame) {
                    std::hint::spin_loop();
                }
            }
        });

        let rb_cons = Arc::clone(&rb);
        let cons = thread::spawn(move || {
            let mut scratch = Vec::new();
            let mut received = 0;
            while received < count {
                match rb_cons.read_frame::<&TelemetryFrame>(&mut scratch) {
                    Ok(Some(frame)) => {
                        assert_eq!(frame.timestamp_ns, received as u64);
                        assert_eq!(frame.tokens_generated, received as u64);
                        received += 1;
                    }
                    Ok(None) => {
                        std::hint::spin_loop();
                    }
                    Err(e) => panic!("Read error: {:?}", e),
                }
            }
            received
        });

        prod.join().unwrap();
        let total = cons.join().unwrap();
        assert_eq!(total, count);
        assert!(rb.is_empty());
    }
}
