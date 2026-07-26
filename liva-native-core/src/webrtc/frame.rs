use bytes::{Buf, BufMut, Bytes, BytesMut};

pub const OP_AUTH_HANDSHAKE: u8 = 0x00;
pub const OP_MIC_IN: u8 = 0x01;
pub const OP_SPEAKER_OUT: u8 = 0x02;
pub const OP_FLUSH: u8 = 0x03;
/// Client báo đã phát xong gói `seq_id`. **Đặt chỗ trong giao thức** —
/// server chưa đọc opcode này (nhánh mặc định nuốt im lặng). Giữ hằng số vì nó
/// là một phần hợp đồng wire mà client có thể gửi; đừng tái dùng giá trị 0x04.
pub const OP_ACK_PLAYING: u8 = 0x04;
/// Client gửi MỘT câu ứng viên để hỏi "đây có phải cụm đánh thức không?".
/// Payload: f32 LE mono 16 kHz, y hệt `OP_MIC_IN`, nhưng là cả câu đã cắt sẵn
/// chứ không phải luồng liên tục. Server xác minh bằng STT + so cụm từ thật rồi
/// trả về sự kiện text `wake_word_triggered` / `wake_probe_rejected`.
///
/// Vì sao cần opcode riêng thay vì cứ nạp `OP_MIC_IN` khi PASSIVE: `OP_MIC_IN`
/// chạy thẳng vào `TurnAudioBuffer` → pipeline → LLM, mà `WakeGate` mặc định là
/// `Off` (`is_awake()` luôn true) nên mọi tiếng động trong phòng sẽ thành một
/// lượt hội thoại thật. Probe là đường cụt: không chạm pipeline, chỉ trả lời.
pub const OP_WAKE_PROBE: u8 = 0x05;

const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
const SPEAKER_PAYLOAD_HEADER_BYTES: usize = 8;
const SPEAKER_FRAME_DURATION_MS: u64 = 100;

#[derive(Debug, Clone)]
pub struct VoiceFrame {
    pub op_code: u8,
    pub seq_id: u32,
    pub payload: Bytes,
}

impl VoiceFrame {
    pub fn encode(&self) -> Result<Bytes, String> {
        if self.payload.len() > MAX_PAYLOAD_BYTES {
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

        if payload_size > MAX_PAYLOAD_BYTES {
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

/// Build bounded OP_SPEAKER_OUT frames without changing the 9-byte VoiceFrame header.
/// Payload contract: [turn_epoch u32 LE][sample_rate u32 LE][f32 LE mono PCM...].
pub fn speaker_frames(turn_epoch: u32, sample_rate: u32, samples: &[f32]) -> Vec<VoiceFrame> {
    if samples.is_empty() {
        return Vec::new();
    }

    let max_samples_per_frame =
        (MAX_PAYLOAD_BYTES - SPEAKER_PAYLOAD_HEADER_BYTES) / std::mem::size_of::<f32>();
    let samples_per_100ms = ((u64::from(sample_rate) * SPEAKER_FRAME_DURATION_MS) / 1000)
        .clamp(1, max_samples_per_frame as u64) as usize;

    samples
        .chunks(samples_per_100ms)
        .enumerate()
        .map(|(seq_id, chunk)| {
            let mut payload =
                Vec::with_capacity(SPEAKER_PAYLOAD_HEADER_BYTES + std::mem::size_of_val(chunk));
            payload.extend_from_slice(&turn_epoch.to_le_bytes());
            payload.extend_from_slice(&sample_rate.to_le_bytes());
            for sample in chunk {
                payload.extend_from_slice(&sample.to_le_bytes());
            }
            VoiceFrame {
                op_code: OP_SPEAKER_OUT,
                seq_id: seq_id as u32,
                payload: Bytes::from(payload),
            }
        })
        .collect()
}

#[derive(Debug, Default)]
pub struct SpeakerEpochGate {
    minimum_epoch: u32,
}

impl SpeakerEpochGate {
    pub fn observe_flush(&mut self, epoch: u32) {
        self.minimum_epoch = self.minimum_epoch.max(epoch);
    }

    pub fn accepts(&self, frame: &VoiceFrame) -> bool {
        if frame.op_code != OP_SPEAKER_OUT {
            return true;
        }
        speaker_turn_epoch(frame).is_some_and(|epoch| epoch >= self.minimum_epoch)
    }
}

pub fn speaker_turn_epoch(frame: &VoiceFrame) -> Option<u32> {
    if frame.op_code != OP_SPEAKER_OUT || frame.payload.len() < 4 {
        return None;
    }
    Some(u32::from_le_bytes(frame.payload[0..4].try_into().ok()?))
}

#[cfg(test)]
mod frame_tests {
    use super::*;

    const MAX: usize = 1024 * 1024;

    fn frame_bytes(op: u8, seq: u32, payload_len_field: u32, payload: &[u8]) -> BytesMut {
        let mut b = BytesMut::new();
        b.put_u8(op);
        b.put_u32_le(seq);
        b.put_u32_le(payload_len_field);
        b.put_slice(payload);
        b
    }

    #[test]
    fn encode_decode_khu_hoi() {
        let f = VoiceFrame {
            op_code: OP_MIC_IN,
            seq_id: 0xDEAD_BEEF,
            payload: Bytes::from_static(&[1, 2, 3, 4]),
        };
        let mut buf = BytesMut::from(&f.encode().unwrap()[..]);
        let out = VoiceFrame::decode(&mut buf).unwrap().unwrap();
        assert_eq!(out.op_code, OP_MIC_IN);
        assert_eq!(out.seq_id, 0xDEAD_BEEF);
        assert_eq!(&out.payload[..], &[1, 2, 3, 4]);
        assert!(buf.is_empty(), "decode phai tieu thu het khung");
    }

    /// Khung chưa đủ phải trả `Ok(None)` để caller chờ thêm byte, KHÔNG được
    /// tiêu thụ buffer — nếu tiêu thụ, dòng byte sẽ lệch vĩnh viễn.
    #[test]
    fn khung_chua_du_khong_duoc_tieu_thu_buffer() {
        for n in 0..9usize {
            let mut b = BytesMut::from(&vec![0u8; n][..]);
            let before = b.len();
            assert!(
                VoiceFrame::decode(&mut b).unwrap().is_none(),
                "{n} byte chua du header"
            );
            assert_eq!(b.len(), before, "khong duoc tieu thu khi chua du");
        }
        // đủ header nhưng thiếu payload
        let mut b = frame_bytes(OP_MIC_IN, 1, 100, &[0u8; 50]);
        let before = b.len();
        assert!(VoiceFrame::decode(&mut b).unwrap().is_none());
        assert_eq!(b.len(), before, "thieu payload cung khong duoc tieu thu");
    }

    /// Đây là ca tấn công: kẻ gửi khai payload_size khổng lồ để ép cấp phát.
    #[test]
    fn payload_size_khong_lo_bi_tu_choi_truoc_khi_cap_phat() {
        for bad in [MAX as u32 + 1, u32::MAX, 0xFFFF_0000] {
            let mut b = frame_bytes(OP_MIC_IN, 0, bad, &[]);
            let r = VoiceFrame::decode(&mut b);
            assert!(r.is_err(), "payload_size {bad} phai bi tu choi");
        }
    }

    #[test]
    fn dung_gioi_han_1mib_van_hop_le() {
        let payload = vec![7u8; MAX];
        let mut b = frame_bytes(OP_SPEAKER_OUT, 5, MAX as u32, &payload);
        let out = VoiceFrame::decode(&mut b).unwrap().unwrap();
        assert_eq!(out.payload.len(), MAX);

        // encode cũng phải chặn đúng ở ngưỡng đó
        let over = VoiceFrame {
            op_code: OP_SPEAKER_OUT,
            seq_id: 0,
            payload: Bytes::from(vec![0u8; MAX + 1]),
        };
        assert!(over.encode().is_err(), "encode phai chan payload > 1MiB");
    }

    #[test]
    fn payload_rong_hop_le() {
        let mut b = frame_bytes(OP_FLUSH, 0, 0, &[]);
        let out = VoiceFrame::decode(&mut b).unwrap().unwrap();
        assert_eq!(out.op_code, OP_FLUSH);
        assert!(out.payload.is_empty());
        assert!(b.is_empty());
    }

    /// Nhiều khung dính liền trong một lần đọc TCP phải tách đúng thứ tự.
    #[test]
    fn nhieu_khung_lien_tiep() {
        let mut b = BytesMut::new();
        for i in 0..3u32 {
            let f = VoiceFrame {
                op_code: OP_MIC_IN,
                seq_id: i,
                payload: Bytes::from(vec![i as u8; 4]),
            };
            b.extend_from_slice(&f.encode().unwrap());
        }
        for i in 0..3u32 {
            let out = VoiceFrame::decode(&mut b).unwrap().unwrap();
            assert_eq!(out.seq_id, i, "phai giu dung thu tu");
            assert_eq!(&out.payload[..], &[i as u8; 4]);
        }
        assert!(VoiceFrame::decode(&mut b).unwrap().is_none());
    }

    /// Opcode lạ KHÔNG được làm decode lỗi — tầng trên mới quyết định xử lý gì.
    /// Nếu decode từ chối, một client gửi opcode tương lai sẽ làm đứt kết nối.
    #[test]
    fn opcode_la_van_decode_duoc() {
        for op in [0x05u8, 0x42, 0xFF] {
            let mut b = frame_bytes(op, 0, 1, &[9]);
            let out = VoiceFrame::decode(&mut b).unwrap().unwrap();
            assert_eq!(out.op_code, op);
        }
    }

    /// Quét thô: mọi chuỗi byte ngắn đều phải trả Ok/Err chứ KHÔNG panic.
    /// `decode` đọc `src[0..9]` bằng chỉ số trần nên đây là chốt chặn thật.
    #[test]
    fn khong_panic_voi_byte_bat_ky() {
        let mut seed = 0x1234_5678u32;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..2000 {
            let len = (next() % 40) as usize;
            let bytes: Vec<u8> = (0..len).map(|_| (next() & 0xFF) as u8).collect();
            let mut b = BytesMut::from(&bytes[..]);
            // Chỉ cần không panic; Ok(None)/Ok(Some)/Err đều chấp nhận được.
            let _ = VoiceFrame::decode(&mut b);
        }
    }

    #[test]
    fn speaker_frames_mang_epoch_va_chia_dung_100ms() {
        let turn_epoch = 42u32;
        let sample_rate = 24_000u32;
        let samples: Vec<f32> = (0..5_000).map(|i| i as f32 * 0.001).collect();

        let frames = speaker_frames(turn_epoch, sample_rate, &samples);

        assert_eq!(
            frames.len(),
            3,
            "5000 mau phai tach thanh 2400 + 2400 + 200"
        );
        for (index, frame) in frames.iter().enumerate() {
            assert_eq!(frame.op_code, OP_SPEAKER_OUT);
            assert_eq!(frame.seq_id, index as u32);
            assert!(
                frame.payload.len() <= MAX,
                "moi chunk phai nam trong wire limit"
            );
            assert_eq!(
                u32::from_le_bytes(frame.payload[0..4].try_into().unwrap()),
                turn_epoch,
            );
            assert_eq!(
                u32::from_le_bytes(frame.payload[4..8].try_into().unwrap()),
                sample_rate,
            );
        }

        assert_eq!(frames[0].payload.len(), 8 + 2_400 * 4);
        assert_eq!(frames[1].payload.len(), 8 + 2_400 * 4);
        assert_eq!(frames[2].payload.len(), 8 + 200 * 4);
    }

    #[test]
    fn speaker_epoch_gate_loai_frame_cua_turn_da_flush() {
        let mut gate = SpeakerEpochGate::default();
        gate.observe_flush(5);
        gate.observe_flush(3);

        let stale = speaker_frames(4, 24_000, &[0.0])[0].clone();
        let current = speaker_frames(5, 24_000, &[0.0])[0].clone();
        let newer = speaker_frames(6, 24_000, &[0.0])[0].clone();

        assert!(!gate.accepts(&stale));
        assert!(gate.accepts(&current));
        assert!(gate.accepts(&newer));
    }
}
