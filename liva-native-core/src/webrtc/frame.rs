use bytes::{Bytes, BytesMut, Buf, BufMut};

pub const OP_AUTH_HANDSHAKE: u8 = 0x00;
pub const OP_MIC_IN: u8 = 0x01;
pub const OP_SPEAKER_OUT: u8 = 0x02;
pub const OP_FLUSH: u8 = 0x03;
pub const OP_ACK_PLAYING: u8 = 0x04;

#[derive(Debug, Clone)]
pub struct VoiceFrame {
    pub op_code: u8,
    pub seq_id: u32,
    pub payload: Bytes,
}

impl VoiceFrame {
    pub fn encode(&self) -> Result<Bytes, String> {
        if self.payload.len() > 1024 * 1024 {
            return Err("Payload exceeds 1MB limit".to_string());
        }
        let mut buf = BytesMut::with_capacity(9 + self.payload.len());
        buf.put_u8(self.op_code);
        buf.put_u32_le(self.seq_id);
        buf.put_u32_le(self.payload.len() as u32);
        buf.put_slice(&self.payload);
        Ok(buf.freeze())
    }

    pub fn decode(src: &mut BytesMut) -> Result<Option<Self>, String> {
        if src.len() < 9 {
            return Ok(None);
        }
        let op_code = src[0];
        let seq_id = u32::from_le_bytes([src[1], src[2], src[3], src[4]]);
        let payload_size = u32::from_le_bytes([src[5], src[6], src[7], src[8]]) as usize;

        if payload_size > 1024 * 1024 {
            return Err("Payload exceeds 1MB limit".to_string());
        }

        if src.len() < 9 + payload_size {
            return Ok(None); // Frame not complete
        }

        src.advance(9);
        let payload = src.split_to(payload_size).freeze();

        Ok(Some(VoiceFrame {
            op_code,
            seq_id,
            payload,
        }))
    }
}
