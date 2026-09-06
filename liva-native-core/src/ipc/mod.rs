//! Zero-Copy IPC Subsystem for `liva-native-core` (RFC-003 Milestone 3).
//!
//! Provides:
//! - Lock-free SPSC (Single-Producer Single-Consumer) ring buffer with 64-byte cache-line alignment (`ring_buffer`).
//! - Zero-copy frame schemas and byte-level codecs with alignment guarantees and validation (`codec`).

pub mod codec;
pub mod ring_buffer;

pub use codec::*;
pub use ring_buffer::*;
