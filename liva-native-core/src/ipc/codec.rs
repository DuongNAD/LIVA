//! Zero-Copy IPC Codecs and Frame Definitions (RFC-003 Milestone 3).
//!
//! Provides ultra-low-latency byte-level serialization and zero-copy deserialization
//! with 8-byte/16-byte alignment guarantees and bounds validation.

use bytemuck::{bytes_of, try_from_bytes, Pod, Zeroable};
use serde::{Deserialize, Serialize};
use std::mem::size_of;

/// Maximum permissible IPC frame payload size (64 MB) to prevent OOM attacks.
pub const MAX_PAYLOAD_SIZE: usize = 64 * 1024 * 1024;

/// Magic identifier bytes for LIVA IPC binary frames: "LIVA" (0x4C, 0x49, 0x56, 0x41).
pub const FRAME_MAGIC: [u8; 4] = *b"LIVA";

/// Current IPC protocol version.
pub const FRAME_VERSION_1: u16 = 1;

/// IPC Error enum covering buffer, serialization, validation, and alignment errors.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum IpcError {
    #[error("Ring buffer capacity exhausted (full)")]
    RingBufferFull,
    #[error("Serialization failure: {0}")]
    Serialization(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Alignment error: expected {expected} byte alignment, got actual {actual}")]
    AlignmentError { expected: usize, actual: usize },
    #[error("Frame size mismatch: expected {expected} bytes, got {actual} bytes")]
    FrameSizeMismatch { expected: usize, actual: usize },
    #[error("Unknown frame type tag: {0}")]
    UnknownFrameType(u16),
    #[error("Checksum mismatch: expected {expected:#010x}, calculated {calculated:#010x}")]
    ChecksumMismatch { expected: u32, calculated: u32 },
    #[error("Payload exceeds maximum permissible size ({size} bytes > {max} bytes limit)")]
    PayloadTooLarge { size: usize, max: usize },
    #[error("Buffer too small: needed {needed} bytes, available {available} bytes")]
    BufferTooSmall { needed: usize, available: usize },
}

/// Identifiers for zero-copy frame payloads.
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FrameType {
    Unknown = 0,
    ScreenDiff = 1,
    AudioStream = 2,
    Telemetry = 3,
    TokenDelta = 4,
    Custom = 5,
}

impl From<u16> for FrameType {
    fn from(val: u16) -> Self {
        match val {
            1 => FrameType::ScreenDiff,
            2 => FrameType::AudioStream,
            3 => FrameType::Telemetry,
            4 => FrameType::TokenDelta,
            5 => FrameType::Custom,
            _ => FrameType::Unknown,
        }
    }
}

/// 8-byte aligned fixed-size header for all LIVA binary IPC frames.
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct FrameHeader {
    pub magic: [u8; 4],   // "LIVA"
    pub version: u16,     // 1
    pub frame_type: u16,  // FrameType
    pub flags: u32,       // Options / compression flags
    pub payload_len: u32, // Length of following payload in bytes
    pub checksum: u32,    // Adler-32 / CRC checksum
    pub _reserved: u32,   // Reserved for future extensions / padding to 24 bytes
}

/// Zero-copy reference to an IPC Frame with validated header and borrowed payload slice.
#[derive(Debug, Clone, PartialEq)]
pub struct IpcFrameRef<'a> {
    pub header: &'a FrameHeader,
    pub payload: &'a [u8],
}

/// Owned IPC Frame container.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcFrame {
    pub frame_type: FrameType,
    pub flags: u32,
    pub payload: Vec<u8>,
}

impl IpcFrame {
    pub fn new(frame_type: FrameType, payload: Vec<u8>) -> Self {
        Self {
            frame_type,
            flags: 0,
            payload,
        }
    }

    /// Serializes the frame into an aligned byte buffer with FrameHeader prefix.
    pub fn encode(&self) -> Vec<u8> {
        let payload_len = self.payload.len();
        let checksum = calculate_checksum(&self.payload);
        let header = FrameHeader {
            magic: FRAME_MAGIC,
            version: FRAME_VERSION_1,
            frame_type: self.frame_type as u16,
            flags: self.flags,
            payload_len: payload_len as u32,
            checksum,
            _reserved: 0,
        };

        let mut buf = vec![0u8; size_of::<FrameHeader>() + payload_len];
        buf[..size_of::<FrameHeader>()].copy_from_slice(bytes_of(&header));
        buf[size_of::<FrameHeader>()..].copy_from_slice(&self.payload);
        buf
    }

    /// Validates and parses an aligned byte buffer in zero-copy mode.
    pub fn decode(bytes: &[u8]) -> Result<IpcFrameRef<'_>, IpcError> {
        if bytes.len() < size_of::<FrameHeader>() {
            return Err(IpcError::FrameSizeMismatch {
                expected: size_of::<FrameHeader>(),
                actual: bytes.len(),
            });
        }

        let header_slice = &bytes[..size_of::<FrameHeader>()];
        let header: &FrameHeader = try_from_bytes(header_slice)
            .map_err(|_| IpcError::Validation("Invalid FrameHeader byte layout".into()))?;

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
        if payload_len > MAX_PAYLOAD_SIZE {
            return Err(IpcError::PayloadTooLarge {
                size: payload_len,
                max: MAX_PAYLOAD_SIZE,
            });
        }

        let expected_total = size_of::<FrameHeader>() + payload_len;
        if bytes.len() < expected_total {
            return Err(IpcError::FrameSizeMismatch {
                expected: expected_total,
                actual: bytes.len(),
            });
        }

        let payload = &bytes[size_of::<FrameHeader>()..expected_total];
        let computed_checksum = calculate_checksum(payload);
        if header.checksum != computed_checksum {
            return Err(IpcError::ChecksumMismatch {
                expected: header.checksum,
                calculated: computed_checksum,
            });
        }

        Ok(IpcFrameRef { header, payload })
    }
}

// -----------------------------------------------------------------------------
// Specialized Zero-Copy Frames
// -----------------------------------------------------------------------------

/// 8-byte aligned header for Screen Difference Frames (Video/GUI Capture).
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct ScreenDiffHeader {
    pub timestamp_ms: i64,
    pub width: u32,
    pub height: u32,
    pub format: u32, // 0: RGBA, 1: BGRA, 2: NV12, 3: JPEG, 4: PNG
    pub damage_x: u32,
    pub damage_y: u32,
    pub damage_w: u32,
    pub damage_h: u32,
    pub data_len: u32,
    pub _pad1: u32,
    pub _pad2: u32,
}

/// Zero-copy view into a ScreenDiffFrame.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenDiffFrameRef<'a> {
    pub header: &'a ScreenDiffHeader,
    pub raw_data: &'a [u8],
}

/// Owned Screen Difference Frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenDiffFrame {
    pub timestamp_ms: i64,
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub damage_x: u32,
    pub damage_y: u32,
    pub damage_w: u32,
    pub damage_h: u32,
    pub raw_data: Vec<u8>,
}

/// 8-byte aligned header for Audio Stream Frames (16kHz PCM / TTS chunks).
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct AudioStreamHeader {
    pub timestamp_ns: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub format: u16, // 0: i16 PCM, 1: f32 PCM, 2: Opus
    pub samples_count: u32,
    pub pcm_bytes_len: u32,
    pub _pad: u64,
}

/// Zero-copy view into an AudioStreamFrame.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioStreamFrameRef<'a> {
    pub header: &'a AudioStreamHeader,
    pub pcm_data: &'a [u8],
}

/// Owned Audio Stream Frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioStreamFrame {
    pub timestamp_ns: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub format: u16,
    pub samples_count: u32,
    pub pcm_data: Vec<u8>,
}

/// Fixed-size zero-copy Telemetry Frame (no dynamic allocation required).
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct TelemetryFrame {
    pub timestamp_ns: u64,
    pub ttft_ns: u64,
    pub total_duration_ns: u64,
    pub tokens_generated: u64,
    pub prompt_tokens: u64,
    pub db_read_latency_ns: u64,
    pub db_write_latency_ns: u64,
    pub memory_rss_bytes: u64,
    pub cpu_usage_percent: f32,
    pub voice_queue_depth: u32,
    pub user_queue_depth: u32,
    pub bg_queue_depth: u32,
    pub preemption_count: u32,
    pub _reserved: u32,
}

/// 8-byte aligned header for Token Stream Delta Frames.
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct TokenDeltaHeader {
    pub task_id: [u8; 16], // UUID as 16 bytes
    pub token_id: i32,
    pub is_first: u8,
    pub is_final: u8,
    pub _reserved1: u16,
    pub cumulative_tokens: u32,
    pub _pad_align: u32,
    pub latency_from_start_ns: u64,
    pub text_len: u32,
    pub _reserved2: u32,
}

/// Zero-copy view into a TokenDeltaFrame.
#[derive(Debug, Clone, PartialEq)]
pub struct TokenDeltaFrameRef<'a> {
    pub header: &'a TokenDeltaHeader,
    pub text: &'a str,
}

/// Owned Token Delta Frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenDeltaFrame {
    pub task_id: uuid::Uuid,
    pub token_id: i32,
    pub is_first: bool,
    pub is_final: bool,
    pub cumulative_tokens: u32,
    pub latency_from_start_ns: u64,
    pub text: String,
}

// -----------------------------------------------------------------------------
// Zero-Copy Serialization & Deserialization Traits
// -----------------------------------------------------------------------------

/// Trait for types that can serialize directly into a pre-allocated byte slice.
pub trait ZeroCopySerializable {
    fn frame_type(&self) -> FrameType;
    fn encoded_len(&self) -> usize;
    fn encode_to_slice(&self, dst: &mut [u8]) -> Result<usize, IpcError>;

    fn to_vec(&self) -> Vec<u8> {
        let mut buf = vec![0u8; self.encoded_len()];
        self.encode_to_slice(&mut buf).expect("Encoding to vec failed");
        buf
    }
}

/// Trait for types that can be deserialized directly from a borrowed byte slice.
pub trait ZeroCopyDeserializable<'a>: Sized {
    fn decode_from_slice(src: &'a [u8]) -> Result<Self, IpcError>;
}

// Implementations for ScreenDiffFrame / ScreenDiffFrameRef

impl ZeroCopySerializable for ScreenDiffFrame {
    fn frame_type(&self) -> FrameType {
        FrameType::ScreenDiff
    }

    fn encoded_len(&self) -> usize {
        size_of::<ScreenDiffHeader>() + self.raw_data.len()
    }

    fn encode_to_slice(&self, dst: &mut [u8]) -> Result<usize, IpcError> {
        let needed = self.encoded_len();
        if dst.len() < needed {
            return Err(IpcError::BufferTooSmall {
                needed,
                available: dst.len(),
            });
        }

        let header = ScreenDiffHeader {
            timestamp_ms: self.timestamp_ms,
            width: self.width,
            height: self.height,
            format: self.format,
            damage_x: self.damage_x,
            damage_y: self.damage_y,
            damage_w: self.damage_w,
            damage_h: self.damage_h,
            data_len: self.raw_data.len() as u32,
            _pad1: 0,
            _pad2: 0,
        };

        dst[..size_of::<ScreenDiffHeader>()].copy_from_slice(bytes_of(&header));
        dst[size_of::<ScreenDiffHeader>()..needed].copy_from_slice(&self.raw_data);
        Ok(needed)
    }
}

impl<'a> ZeroCopyDeserializable<'a> for ScreenDiffFrameRef<'a> {
    fn decode_from_slice(src: &'a [u8]) -> Result<Self, IpcError> {
        if src.len() < size_of::<ScreenDiffHeader>() {
            return Err(IpcError::FrameSizeMismatch {
                expected: size_of::<ScreenDiffHeader>(),
                actual: src.len(),
            });
        }

        let header_slice = &src[..size_of::<ScreenDiffHeader>()];
        let header: &ScreenDiffHeader = try_from_bytes(header_slice)
            .map_err(|_| IpcError::Validation("Invalid ScreenDiffHeader layout".into()))?;

        let data_len = header.data_len as usize;
        let expected_total = size_of::<ScreenDiffHeader>() + data_len;
        if src.len() < expected_total {
            return Err(IpcError::FrameSizeMismatch {
                expected: expected_total,
                actual: src.len(),
            });
        }

        let raw_data = &src[size_of::<ScreenDiffHeader>()..expected_total];
        Ok(ScreenDiffFrameRef { header, raw_data })
    }
}

// Implementations for AudioStreamFrame / AudioStreamFrameRef

impl ZeroCopySerializable for AudioStreamFrame {
    fn frame_type(&self) -> FrameType {
        FrameType::AudioStream
    }

    fn encoded_len(&self) -> usize {
        size_of::<AudioStreamHeader>() + self.pcm_data.len()
    }

    fn encode_to_slice(&self, dst: &mut [u8]) -> Result<usize, IpcError> {
        let needed = self.encoded_len();
        if dst.len() < needed {
            return Err(IpcError::BufferTooSmall {
                needed,
                available: dst.len(),
            });
        }

        let header = AudioStreamHeader {
            timestamp_ns: self.timestamp_ns,
            sample_rate: self.sample_rate,
            channels: self.channels,
            format: self.format,
            samples_count: self.samples_count,
            pcm_bytes_len: self.pcm_data.len() as u32,
            _pad: 0,
        };

        dst[..size_of::<AudioStreamHeader>()].copy_from_slice(bytes_of(&header));
        dst[size_of::<AudioStreamHeader>()..needed].copy_from_slice(&self.pcm_data);
        Ok(needed)
    }
}

impl<'a> ZeroCopyDeserializable<'a> for AudioStreamFrameRef<'a> {
    fn decode_from_slice(src: &'a [u8]) -> Result<Self, IpcError> {
        if src.len() < size_of::<AudioStreamHeader>() {
            return Err(IpcError::FrameSizeMismatch {
                expected: size_of::<AudioStreamHeader>(),
                actual: src.len(),
            });
        }

        let header_slice = &src[..size_of::<AudioStreamHeader>()];
        let header: &AudioStreamHeader = try_from_bytes(header_slice)
            .map_err(|_| IpcError::Validation("Invalid AudioStreamHeader layout".into()))?;

        let pcm_len = header.pcm_bytes_len as usize;
        let expected_total = size_of::<AudioStreamHeader>() + pcm_len;
        if src.len() < expected_total {
            return Err(IpcError::FrameSizeMismatch {
                expected: expected_total,
                actual: src.len(),
            });
        }

        let pcm_data = &src[size_of::<AudioStreamHeader>()..expected_total];
        Ok(AudioStreamFrameRef { header, pcm_data })
    }
}

// Implementations for TelemetryFrame

impl ZeroCopySerializable for TelemetryFrame {
    fn frame_type(&self) -> FrameType {
        FrameType::Telemetry
    }

    fn encoded_len(&self) -> usize {
        size_of::<TelemetryFrame>()
    }

    fn encode_to_slice(&self, dst: &mut [u8]) -> Result<usize, IpcError> {
        let needed = self.encoded_len();
        if dst.len() < needed {
            return Err(IpcError::BufferTooSmall {
                needed,
                available: dst.len(),
            });
        }
        dst[..needed].copy_from_slice(bytes_of(self));
        Ok(needed)
    }
}

impl<'a> ZeroCopyDeserializable<'a> for &'a TelemetryFrame {
    fn decode_from_slice(src: &'a [u8]) -> Result<Self, IpcError> {
        if src.len() < size_of::<TelemetryFrame>() {
            return Err(IpcError::FrameSizeMismatch {
                expected: size_of::<TelemetryFrame>(),
                actual: src.len(),
            });
        }

        try_from_bytes(&src[..size_of::<TelemetryFrame>()])
            .map_err(|_| IpcError::Validation("Invalid TelemetryFrame layout or alignment".into()))
    }
}

// Implementations for TokenDeltaFrame / TokenDeltaFrameRef

impl ZeroCopySerializable for TokenDeltaFrame {
    fn frame_type(&self) -> FrameType {
        FrameType::TokenDelta
    }

    fn encoded_len(&self) -> usize {
        size_of::<TokenDeltaHeader>() + self.text.as_bytes().len()
    }

    fn encode_to_slice(&self, dst: &mut [u8]) -> Result<usize, IpcError> {
        let text_bytes = self.text.as_bytes();
        let needed = self.encoded_len();
        if dst.len() < needed {
            return Err(IpcError::BufferTooSmall {
                needed,
                available: dst.len(),
            });
        }

        let header = TokenDeltaHeader {
            task_id: *self.task_id.as_bytes(),
            token_id: self.token_id,
            is_first: if self.is_first { 1 } else { 0 },
            is_final: if self.is_final { 1 } else { 0 },
            _reserved1: 0,
            cumulative_tokens: self.cumulative_tokens,
            _pad_align: 0,
            latency_from_start_ns: self.latency_from_start_ns,
            text_len: text_bytes.len() as u32,
            _reserved2: 0,
        };

        dst[..size_of::<TokenDeltaHeader>()].copy_from_slice(bytes_of(&header));
        dst[size_of::<TokenDeltaHeader>()..needed].copy_from_slice(text_bytes);
        Ok(needed)
    }
}

impl<'a> ZeroCopyDeserializable<'a> for TokenDeltaFrameRef<'a> {
    fn decode_from_slice(src: &'a [u8]) -> Result<Self, IpcError> {
        if src.len() < size_of::<TokenDeltaHeader>() {
            return Err(IpcError::FrameSizeMismatch {
                expected: size_of::<TokenDeltaHeader>(),
                actual: src.len(),
            });
        }

        let header_slice = &src[..size_of::<TokenDeltaHeader>()];
        let header: &TokenDeltaHeader = try_from_bytes(header_slice)
            .map_err(|_| IpcError::Validation("Invalid TokenDeltaHeader layout".into()))?;

        let text_len = header.text_len as usize;
        let expected_total = size_of::<TokenDeltaHeader>() + text_len;
        if src.len() < expected_total {
            return Err(IpcError::FrameSizeMismatch {
                expected: expected_total,
                actual: src.len(),
            });
        }

        let text_slice = &src[size_of::<TokenDeltaHeader>()..expected_total];
        let text = std::str::from_utf8(text_slice)
            .map_err(|e| IpcError::Validation(format!("Invalid UTF-8 in TokenDeltaFrame: {}", e)))?;

        Ok(TokenDeltaFrameRef { header, text })
    }
}

// -----------------------------------------------------------------------------
// Checksum Calculation (Adler-32)
// -----------------------------------------------------------------------------

/// Computes an Adler-32 checksum in sub-microsecond time over the payload slice.
#[inline]
pub fn calculate_checksum(data: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65521;
    let mut a: u32 = 1;
    let mut b: u32 = 0;

    for chunk in data.chunks(5552) {
        for &byte in chunk {
            a = a.wrapping_add(byte as u32);
            b = b.wrapping_add(a);
        }
        a %= MOD_ADLER;
        b %= MOD_ADLER;
    }

    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_frame_header_alignment_and_size() {
        assert_eq!(std::mem::align_of::<FrameHeader>(), 8);
        assert_eq!(std::mem::size_of::<FrameHeader>(), 24);
        assert_eq!(std::mem::align_of::<ScreenDiffHeader>(), 8);
        assert_eq!(std::mem::align_of::<AudioStreamHeader>(), 8);
        assert_eq!(std::mem::align_of::<TelemetryFrame>(), 8);
        assert_eq!(std::mem::align_of::<TokenDeltaHeader>(), 8);
    }

    #[test]
    fn test_ipc_frame_encode_decode() {
        let payload = b"Zero-Copy IPC Payload Verification";
        let frame = IpcFrame::new(FrameType::Custom, payload.to_vec());
        let encoded = frame.encode();

        let decoded = IpcFrame::decode(&encoded).expect("Decode should succeed");
        assert_eq!(decoded.header.magic, FRAME_MAGIC);
        assert_eq!(decoded.header.version, FRAME_VERSION_1);
        assert_eq!(decoded.header.frame_type, FrameType::Custom as u16);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_screen_diff_frame_zero_copy() {
        let raw_pixels = vec![255u8; 1920 * 1080 * 4]; // 8.29 MB 1080p frame
        let frame = ScreenDiffFrame {
            timestamp_ms: 1725200000,
            width: 1920,
            height: 1080,
            format: 0,
            damage_x: 0,
            damage_y: 0,
            damage_w: 1920,
            damage_h: 1080,
            raw_data: raw_pixels,
        };

        let encoded = frame.to_vec();

        let start = Instant::now();
        let decoded = ScreenDiffFrameRef::decode_from_slice(&encoded).expect("Decode slice");
        let elapsed = start.elapsed();

        assert_eq!(decoded.header.width, 1920);
        assert_eq!(decoded.header.height, 1080);
        assert_eq!(decoded.raw_data.len(), 1920 * 1080 * 4);
        // Zero-copy deserialization is pure pointer cast, should be < 50 microseconds even in debug mode
        println!("ScreenDiffFrame 8.3MB zero-copy decode time: {:?}", elapsed);
    }

    #[test]
    fn test_audio_stream_frame_zero_copy() {
        let pcm = vec![0x12u8; 32000]; // 1 second of 16kHz 16-bit audio
        let frame = AudioStreamFrame {
            timestamp_ns: 1_000_000_000,
            sample_rate: 16000,
            channels: 1,
            format: 0,
            samples_count: 16000,
            pcm_data: pcm.clone(),
        };

        let encoded = frame.to_vec();
        let decoded = AudioStreamFrameRef::decode_from_slice(&encoded).expect("Decode audio");
        assert_eq!(decoded.header.sample_rate, 16000);
        assert_eq!(decoded.pcm_data, &pcm[..]);
    }

    #[test]
    fn test_telemetry_frame_zero_copy_access() {
        let telemetry = TelemetryFrame {
            timestamp_ns: 1_700_000_000,
            ttft_ns: 24_500_000, // 24.5ms
            total_duration_ns: 1_150_000_000,
            tokens_generated: 45,
            prompt_tokens: 120,
            db_read_latency_ns: 150_000,
            db_write_latency_ns: 250_000,
            memory_rss_bytes: 32 * 1024 * 1024,
            cpu_usage_percent: 12.5,
            voice_queue_depth: 0,
            user_queue_depth: 1,
            bg_queue_depth: 2,
            preemption_count: 3,
            _reserved: 0,
        };

        let encoded = telemetry.to_vec();

        let start = Instant::now();
        let decoded_ref: &TelemetryFrame =
            <&TelemetryFrame>::decode_from_slice(&encoded).expect("Decode telemetry");
        let elapsed = start.elapsed();

        assert_eq!(decoded_ref.ttft_ns, 24_500_000);
        assert_eq!(decoded_ref.tokens_generated, 45);
        assert_eq!(decoded_ref.cpu_usage_percent, 12.5);
        println!("TelemetryFrame zero-copy decode time: {:?}", elapsed);
    }

    #[test]
    fn test_token_delta_frame_zero_copy() {
        let task_id = uuid::Uuid::new_v4();
        let text = "Xin chào LIVA!";
        let frame = TokenDeltaFrame {
            task_id,
            token_id: 42,
            is_first: true,
            is_final: false,
            cumulative_tokens: 1,
            latency_from_start_ns: 15_000_000,
            text: text.to_string(),
        };

        let encoded = frame.to_vec();
        let decoded = TokenDeltaFrameRef::decode_from_slice(&encoded).expect("Decode token delta");
        assert_eq!(decoded.header.token_id, 42);
        assert_eq!(decoded.header.is_first, 1);
        assert_eq!(decoded.text, text);
    }

    #[test]
    fn test_checksum_tampering_detection() {
        let frame = IpcFrame::new(FrameType::Custom, b"Authentic Data".to_vec());
        let mut encoded = frame.encode();

        // Tamper with payload
        let last_idx = encoded.len() - 1;
        encoded[last_idx] ^= 0xFF;

        let err = IpcFrame::decode(&encoded).unwrap_err();
        assert!(matches!(err, IpcError::ChecksumMismatch { .. }));
    }

    #[test]
    fn test_invalid_magic_rejection() {
        let frame = IpcFrame::new(FrameType::Custom, b"Test Data".to_vec());
        let mut encoded = frame.encode();
        encoded[0] = b'X'; // Corrupt magic

        let err = IpcFrame::decode(&encoded).unwrap_err();
        assert!(matches!(err, IpcError::Validation(_)));
    }

    #[test]
    fn test_invalid_version_rejection() {
        let frame = IpcFrame::new(FrameType::Custom, b"Test Data".to_vec());
        let mut encoded = frame.encode();
        encoded[4] = 99; // Corrupt version to 99

        let err = IpcFrame::decode(&encoded).unwrap_err();
        assert!(matches!(err, IpcError::Validation(_)));
    }

    #[test]
    fn test_truncated_frame_rejection() {
        let frame = IpcFrame::new(FrameType::Custom, b"Test Data".to_vec());
        let encoded = frame.encode();
        // Truncate to less than header size
        let truncated = &encoded[..10];
        let err = IpcFrame::decode(truncated).unwrap_err();
        assert!(matches!(err, IpcError::FrameSizeMismatch { .. }));

        // Truncate payload
        let truncated_payload = &encoded[..encoded.len() - 2];
        let err2 = IpcFrame::decode(truncated_payload).unwrap_err();
        assert!(matches!(err2, IpcError::FrameSizeMismatch { .. }));
    }

    #[test]
    fn test_zero_copy_deserialization_sla_latency() {
        // 1MB ScreenDiffFrame for SLA latency check
        let data = vec![128u8; 1024 * 1024];
        let frame = ScreenDiffFrame {
            timestamp_ms: 1000,
            width: 1024,
            height: 1024,
            format: 0,
            damage_x: 0,
            damage_y: 0,
            damage_w: 1024,
            damage_h: 1024,
            raw_data: data,
        };
        let encoded = frame.to_vec();

        // Warm up
        for _ in 0..100 {
            let _ = ScreenDiffFrameRef::decode_from_slice(&encoded).unwrap();
        }

        // Measure 10,000 decode iterations
        let iterations = 10_000;
        let start = Instant::now();
        for _ in 0..iterations {
            let decoded = ScreenDiffFrameRef::decode_from_slice(&encoded).unwrap();
            assert_eq!(decoded.header.width, 1024);
        }
        let total_duration = start.elapsed();
        let per_op_ns = total_duration.as_nanos() as f64 / iterations as f64;
        let per_op_us = per_op_ns / 1000.0;

        println!(
            "1MB Zero-Copy Deserialization Latency: {:.3} ns ({:.4} µs) per operation",
            per_op_ns, per_op_us
        );

        // Deserialization of 1MB must be <= 25 µs (target SLA)
        assert!(
            per_op_us <= 25.0,
            "Zero-copy deserialization latency SLA violation: {:.4} µs > 25.0 µs",
            per_op_us
        );
    }
}
