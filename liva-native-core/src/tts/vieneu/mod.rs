//! VieNeu-TTS v3 Turbo — native Rust `ort` engine (CPU, torch-free).
//!
//! A faithful port of VieNeu-TTS's own torch-free reference engine
//! (`onnx_runtime_lite.py`, Apache-2.0). VieNeu is an autoregressive
//! LLM + neural-codec TTS: a Qwen3 backbone emits one hidden state per audio
//! frame; a 1-layer acoustic decoder turns each hidden into the 16 residual-VQ
//! codes of one 12.5 Hz frame; the MOSS-Audio-Tokenizer-Nano codec decodes the
//! code stream to a 48 kHz waveform. The transformer maths live inside the ONNX
//! graphs — this module only feeds `inputs_embeds`/KV-cache tensors and does the
//! embedding lookups, output-head mat-muls, sampling and the decode loop in Rust.
//!
//! Unlike Piper (single-pass VITS), one utterance is ~5k small ONNX runs, so this
//! is the "premium" quality tier: Piper stays the always-on/under-load voice.
//!
//! Preset voices carry a precomputed 192-d speaker embedding + in-context ref
//! codes, so synthesis needs no speaker-encoder/denoiser — those (live cloning
//! from a wav) are a follow-up. Model dir layout (see `models/vieneu/`):
//!   vieneu_prefill.onnx, vieneu_decode_step.onnx, vieneu_acoustic_cached.onnx,
//!   vieneu_backbone_shared.data, vieneu_v3_heads.npz, config.json,
//!   tokenizer.json, voices_v3_turbo.json, sea_g2p.bin,
//!   moss_audio_tokenizer_decode_full.onnx (+ .data).

mod g2p;
mod punc;

use g2p::G2PEngine;
use ndarray::{Array0, Array1, Array2, Array3, ArrayView1, Axis};
use ndarray_npy::NpzReader;
use ort::{session::Session, value::Value};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::path::Path;
use tokenizers::Tokenizer;

// Default sampling parameters (match VieNeu's `infer` defaults).
const TEMPERATURE: f32 = 0.8;
const TOP_K: usize = 25;
const TOP_P: f32 = 0.95;
const REP_PEN: f32 = 1.2;
const MAX_NEW_FRAMES: usize = 300; // ~24 s ceiling @ 12.5 Hz

/// Token ids + architecture sizes, read from `config.json`.
struct Cfg {
    n_vq: usize,
    hidden: usize,
    n_layers: usize,
    kv_heads: usize,
    head_dim: usize,
    loc_heads: usize,
    loc_head_dim: usize,
    audio_pad: i64,
    tps: i64,
    tpe: i64,
    sgs: i64,
    eos_speech: i64,
    ref_slot: i64,
}

impl Cfg {
    fn from_json(v: &serde_json::Value) -> Result<Self, String> {
        let get_i64 = |k: &str| -> Result<i64, String> {
            v.get(k)
                .and_then(|x| x.as_i64())
                .ok_or_else(|| format!("config.json missing integer field '{}'", k))
        };
        let get_positive_usize = |k: &str| -> Result<usize, String> {
            let value = get_i64(k)?;
            if value <= 0 {
                return Err(format!("config.json field '{k}' must be positive"));
            }
            usize::try_from(value)
                .map_err(|_| format!("config.json field '{k}' is too large for this platform"))
        };
        let get_token_id = |k: &str| -> Result<i64, String> {
            let value = get_i64(k)?;
            if value < 0 {
                return Err(format!("config.json token id '{k}' must not be negative"));
            }
            Ok(value)
        };

        let hidden = get_positive_usize("hidden_size")?;
        let loc_heads = get_positive_usize("local_num_attention_heads")?;
        if hidden % loc_heads != 0 {
            return Err(format!(
                "config.json hidden_size {hidden} is not divisible by local_num_attention_heads {loc_heads}"
            ));
        }
        Ok(Self {
            n_vq: get_positive_usize("n_vq")?,
            hidden,
            n_layers: get_positive_usize("num_hidden_layers")?,
            kv_heads: get_positive_usize("num_key_value_heads")?,
            head_dim: get_positive_usize("head_dim")?,
            loc_heads,
            loc_head_dim: hidden / loc_heads,
            audio_pad: get_token_id("audio_pad_token_id")?,
            tps: get_token_id("text_prompt_start_token_id")?,
            tpe: get_token_id("text_prompt_end_token_id")?,
            sgs: get_token_id("speech_generation_start_token_id")?,
            eos_speech: get_token_id("speech_generation_end_token_id")?,
            ref_slot: get_token_id("audio_ref_slot_token_id")?,
        })
    }
}

/// A loaded VieNeu voice: the four ONNX sessions, the tied embedding/head
/// weights, and one selected speaker (anchor + in-context ref codes).
pub struct VieNeuVoice {
    sess_pre: Session,
    sess_dec: Session,
    sess_ac: Session,
    sess_codec: Session,

    text_emb: Array2<f32>,  // (Vt, H)
    audio_emb: Array3<f32>, // (n_vq, Va, H)
    anchor: Array1<f32>,    // (H,) precomputed for the selected voice

    cfg: Cfg,
    style_id: i64,
    ref_codes: Vec<Vec<i64>>, // (T_ref, n_vq), empty if none

    g2p: G2PEngine,
    tokenizer: Tokenizer,
    rng: StdRng,
    voice_name: String,
    sample_rate: u32,
}

impl VieNeuVoice {
    /// Load the engine from a model directory. `voice` picks a preset by name;
    /// `None` uses the `default_voice` from `voices_v3_turbo.json`.
    pub fn load(model_dir: &Path, voice: Option<&str>) -> Result<Self, String> {
        let need = |name: &str| -> std::path::PathBuf { model_dir.join(name) };
        let read = |name: &str| -> Result<String, String> {
            std::fs::read_to_string(need(name)).map_err(|e| format!("read {}: {}", name, e))
        };

        // ── config ─────────────────────────────────────────────────────────
        let cfg_json: serde_json::Value = serde_json::from_str(&read("config.json")?)
            .map_err(|e| format!("parse config.json: {}", e))?;
        let cfg = Cfg::from_json(&cfg_json)?;
        let intra = std::env::var("LIVA_VIENEU_THREADS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(4);

        // ── tied embeddings / heads / xvec_proj (npz) ──────────────────────
        let mut npz = NpzReader::new(
            std::fs::File::open(need("vieneu_v3_heads.npz"))
                .map_err(|e| format!("open heads npz: {}", e))?,
        )
        .map_err(|e| format!("read heads npz: {}", e))?;
        let names = npz.names().map_err(|e| format!("npz names: {}", e))?;
        let entry = |base: &str| -> Result<String, String> {
            let withext = format!("{base}.npy");
            if names.iter().any(|n| n == &withext) {
                Ok(withext)
            } else if names.iter().any(|n| n == base) {
                Ok(base.to_string())
            } else {
                Err(format!("heads npz missing '{base}'"))
            }
        };
        let text_emb: Array2<f32> = npz
            .by_name(&entry("text_emb")?)
            .map_err(|e| format!("read text_emb: {}", e))?;
        let audio_emb: Array3<f32> = npz
            .by_name(&entry("audio_emb")?)
            .map_err(|e| format!("read audio_emb: {}", e))?;
        let xvec_w: Array2<f32> = npz
            .by_name(&entry("xvec_w")?)
            .map_err(|e| format!("read xvec_w: {}", e))?;
        let xvec_b: Array1<f32> = npz
            .by_name(&entry("xvec_b")?)
            .map_err(|e| format!("read xvec_b: {}", e))?;
        let xvec_ln_w: Array1<f32> = npz
            .by_name(&entry("xvec_ln_w")?)
            .map_err(|e| format!("read xvec_ln_w: {}", e))?;
        let xvec_ln_b: Array1<f32> = npz
            .by_name(&entry("xvec_ln_b")?)
            .map_err(|e| format!("read xvec_ln_b: {}", e))?;
        let xvec_ln_eps: Array0<f32> = npz
            .by_name(&entry("xvec_ln_eps")?)
            .map_err(|e| format!("read xvec_ln_eps: {}", e))?;
        let xvec_ln_eps = xvec_ln_eps.into_scalar();

        // ── selected voice (speaker_emb → anchor, ref codes, style) ────────
        let voices_json: serde_json::Value = serde_json::from_str(&read("voices_v3_turbo.json")?)
            .map_err(|e| format!("parse voices json: {}", e))?;
        let (voice_name, speaker_emb, ref_codes, style_id) =
            select_voice(&voices_json, &cfg_json, voice)?;
        let anchor = speaker_anchor(
            &speaker_emb,
            &xvec_w,
            &xvec_b,
            &xvec_ln_w,
            &xvec_ln_b,
            xvec_ln_eps,
        )?;

        // ── tokenizer + phonemizer ─────────────────────────────────────────
        let tokenizer = Tokenizer::from_file(need("tokenizer.json"))
            .map_err(|e| format!("load tokenizer.json: {}", e))?;
        let g2p = G2PEngine::new(
            need("sea_g2p.bin")
                .to_str()
                .ok_or("sea_g2p.bin path not UTF-8")?,
        )
        .map_err(|e| format!("load sea_g2p.bin: {}", e))?;

        // ── ONNX sessions (default CPU EP, same idiom as the other engines) ─
        let build = |name: &str| -> Result<Session, String> {
            Session::builder()
                .map_err(|e| e.to_string())?
                .with_intra_threads(intra)
                .map_err(|e| e.to_string())?
                .with_inter_threads(1)
                .map_err(|e| e.to_string())?
                .commit_from_file(need(name))
                .map_err(|e| format!("load {}: {}", name, e))
        };
        let sess_pre = build("vieneu_prefill.onnx")?;
        let sess_dec = build("vieneu_decode_step.onnx")?;
        let sess_ac = build("vieneu_acoustic_cached.onnx")?;
        let sess_codec = build("moss_audio_tokenizer_decode_full.onnx")?;

        let seed = std::env::var("LIVA_VIENEU_SEED")
            .ok()
            .and_then(|s| s.parse::<u64>().ok());
        let rng = match seed {
            Some(s) => StdRng::seed_from_u64(s),
            None => StdRng::from_entropy(),
        };

        tracing::info!(
            "VieNeu-TTS loaded (voice='{}', style_id={}, {} ref frames)",
            voice_name,
            style_id,
            ref_codes.len()
        );

        Ok(Self {
            sess_pre,
            sess_dec,
            sess_ac,
            sess_codec,
            text_emb,
            audio_emb,
            anchor,
            cfg,
            style_id,
            ref_codes,
            g2p,
            tokenizer,
            rng,
            voice_name,
            sample_rate: 48_000,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn voice_name(&self) -> &str {
        &self.voice_name
    }

    /// Synthesize UTF-8 text (already number/date-normalized upstream) to mono
    /// f32 samples at [`Self::sample_rate`] (48 kHz).
    pub fn synthesize(&mut self, text: &str) -> Result<Vec<f32>, String> {
        let phonemes = self.g2p.phonemize(&punc::apply_punc_norm(text));
        if phonemes.trim().is_empty() {
            return Err(format!("no phonemes produced for text: {:?}", text));
        }

        // ── build the (T, n_vq+1) prompt rows ──────────────────────────────
        let enc = self
            .tokenizer
            .encode(phonemes.as_str(), false)
            .map_err(|e| format!("tokenize phonemes: {}", e))?;
        let phone_ids: Vec<i64> = enc.get_ids().iter().map(|&id| id as i64).collect();

        let n_col = self.cfg.n_vq + 1;
        let pad = self.cfg.audio_pad;
        let mut rows: Vec<Vec<i64>> = Vec::new();
        // text rows: [style, tps] + phones + [tpe] in column 0, pad elsewhere.
        let mut text_ids = Vec::with_capacity(phone_ids.len() + 3);
        text_ids.push(self.style_id);
        text_ids.push(self.cfg.tps);
        text_ids.extend_from_slice(&phone_ids);
        text_ids.push(self.cfg.tpe);
        for tid in text_ids {
            let mut row = vec![pad; n_col];
            row[0] = tid;
            rows.push(row);
        }
        // in-context reference rows: col0 = ref_slot, cols 1.. = ref codes.
        for rc in &self.ref_codes {
            let mut row = vec![pad; n_col];
            row[0] = self.cfg.ref_slot;
            for (i, &c) in rc.iter().enumerate().take(self.cfg.n_vq) {
                row[i + 1] = c;
            }
            rows.push(row);
        }
        let t_prompt = rows.len();
        let prompt_embeds = self.embed_rows(&rows)?; // (T*H) row-major

        // ── prefill ────────────────────────────────────────────────────────
        let h_dim = self.cfg.hidden;
        let pre_inputs = ort::inputs![
            "inputs_embeds" => Value::from_array((vec![1usize, t_prompt, h_dim], prompt_embeds))
                .map_err(|e| e.to_string())?,
        ];
        let pre = self
            .sess_pre
            .run(pre_inputs)
            .map_err(|e| format!("prefill run: {}", e))?;
        let mut past_k: Vec<Vec<f32>> = Vec::with_capacity(self.cfg.n_layers);
        let mut past_v: Vec<Vec<f32>> = Vec::with_capacity(self.cfg.n_layers);
        for i in 0..self.cfg.n_layers {
            past_k.push(extract_f32(&pre, &format!("present_k_{i}"))?);
            past_v.push(extract_f32(&pre, &format!("present_v_{i}"))?);
        }
        let hidden_full = extract_f32(&pre, "hidden")?;
        // last prompt position's hidden state (1, T, H) → row T-1
        let hidden_start = (t_prompt - 1)
            .checked_mul(h_dim)
            .ok_or_else(|| "prefill hidden offset overflow".to_string())?;
        let mut h =
            checked_tensor_window(&hidden_full, hidden_start, h_dim, "prefill hidden")?.to_vec();
        let mut past_len = t_prompt;
        drop(pre);

        // ── autoregressive frame loop ──────────────────────────────────────
        let mut hist: Vec<std::collections::HashSet<i64>> =
            vec![std::collections::HashSet::new(); self.cfg.n_vq];
        let mut frames: Vec<Vec<i64>> = Vec::new();
        // past_len là SỐ VỊ TRÍ ĐÃ CACHE trong KV — một khái niệm của decode
        // tự hồi quy, không phải biến đếm vòng lặp. Dạng zip clippy gợi ý
        // ((t_prompt..).zip(..)) tương đương về giá trị nhưng xoá khái niệm đó
        // khỏi mặt chữ, ở đúng chỗ sổ sách KV dễ sai nhất.
        #[allow(clippy::explicit_counter_loop)]
        for t in 0..MAX_NEW_FRAMES {
            let (codes, eos) = self.acoustic_frame(&h, &mut hist)?;
            frames.push(codes.clone());
            if eos {
                break;
            }
            // Feed the sampled frame back into the backbone.
            let mut slot = vec![pad; n_col];
            slot[0] = self.cfg.sgs;
            for (i, &c) in codes.iter().enumerate() {
                slot[i + 1] = c;
            }
            let se = self.embed_rows(std::slice::from_ref(&slot))?; // (1*H)
            let pos = (t_prompt + t) as i64;
            let mut feed = ort::inputs![
                "inputs_embeds" => Value::from_array((vec![1usize, 1, h_dim], se))
                    .map_err(|e| e.to_string())?,
                "position_ids" => Value::from_array((vec![1usize, 1], vec![pos]))
                    .map_err(|e| e.to_string())?,
            ];
            // `std::mem::take` thay vì `.clone()`: `Value::from_array` cần sở
            // hữu buffer, mà ngay sau `run()` ta ghi đè `past_k[i]`/`past_v[i]`
            // bằng `present_*` nên bản cũ không còn ai dùng. Clone ở đây là
            // sao chép `kv_heads × past_len × head_dim` float cho MỖI lớp ở MỖI
            // bước decode — tổng khối lượng bậc hai theo độ dài chuỗi.
            for i in 0..self.cfg.n_layers {
                let shape = vec![1usize, self.cfg.kv_heads, past_len, self.cfg.head_dim];
                feed.push((
                    format!("past_k_{i}").into(),
                    Value::from_array((shape.clone(), std::mem::take(&mut past_k[i])))
                        .map_err(|e| e.to_string())?
                        .into(),
                ));
                feed.push((
                    format!("past_v_{i}").into(),
                    Value::from_array((shape, std::mem::take(&mut past_v[i])))
                        .map_err(|e| e.to_string())?
                        .into(),
                ));
            }
            let out = self
                .sess_dec
                .run(feed)
                .map_err(|e| format!("decode_step run: {}", e))?;
            let hd = extract_f32(&out, "hidden")?;
            h = hd[0..h_dim].to_vec();
            for i in 0..self.cfg.n_layers {
                past_k[i] = extract_f32(&out, &format!("present_k_{i}"))?;
                past_v[i] = extract_f32(&out, &format!("present_v_{i}"))?;
            }
            past_len += 1;
        }

        if frames.is_empty() {
            return Ok(Vec::new());
        }
        self.decode_codes(&frames)
    }

    /// rows: each is `[text_or_slot_id, code_0..code_{n_vq-1}]`. Returns a
    /// row-major `(rows.len() * H)` embedding tensor (mirror `_embed_rows`).
    fn embed_rows(&self, rows: &[Vec<i64>]) -> Result<Vec<f32>, String> {
        let h = self.cfg.hidden;
        if self.text_emb.ncols() != h {
            return Err(format!(
                "text embedding width {} does not match hidden size {h}",
                self.text_emb.ncols()
            ));
        }
        let audio_shape = self.audio_emb.shape();
        if audio_shape[0] != self.cfg.n_vq || audio_shape[2] != h {
            return Err(format!(
                "audio embedding shape {:?} does not match ({}, vocab, {h})",
                audio_shape, self.cfg.n_vq
            ));
        }
        if self.anchor.len() != h {
            return Err(format!(
                "speaker anchor length {} does not match hidden size {h}",
                self.anchor.len()
            ));
        }
        let output_len = rows
            .len()
            .checked_mul(h)
            .ok_or_else(|| "embedding output length overflow".to_string())?;
        let mut out = vec![0.0f32; output_len];
        for (t, row) in rows.iter().enumerate() {
            if row.len() != self.cfg.n_vq + 1 {
                return Err(format!(
                    "embedding row {t} has {} columns; expected {}",
                    row.len(),
                    self.cfg.n_vq + 1
                ));
            }
            let dst = &mut out[t * h..(t + 1) * h];
            // text/slot embedding (column 0)
            let text_id = checked_embedding_index(row[0], self.text_emb.nrows(), "text token")?;
            let te = self.text_emb.row(text_id);
            for (d, &s) in dst.iter_mut().zip(te.iter()) {
                *d = s;
            }
            // per-codebook audio embeddings (columns 1..=n_vq)
            for ch in 0..self.cfg.n_vq {
                let id = row[ch + 1];
                if id != self.cfg.audio_pad {
                    let ae = self.audio_emb.index_axis(Axis(0), ch);
                    let code_id = checked_embedding_index(id, ae.len_of(Axis(0)), "audio code")?;
                    let ae = ae.index_axis(Axis(0), code_id);
                    for (d, &s) in dst.iter_mut().zip(ae.iter()) {
                        *d += s;
                    }
                }
            }
            // speaker anchor (same on every row)
            for (d, &a) in dst.iter_mut().zip(self.anchor.iter()) {
                *d += a;
            }
        }
        Ok(out)
    }

    /// One audio frame: run the acoustic decoder over its 16 codebooks and
    /// probe the EOS logit (mirror `_acoustic_frame`). Returns (16 codes, eos).
    fn acoustic_frame(
        &mut self,
        h: &[f32],
        hist: &mut [std::collections::HashSet<i64>],
    ) -> Result<(Vec<i64>, bool), String> {
        let hdim = self.cfg.hidden;
        let loc_heads = self.cfg.loc_heads;
        let loc_hd = self.cfg.loc_head_dim;

        // Seed step: token_emb = [cond=h, txt=text_emb[sgs]], positions [0,1].
        let mut tok = Vec::with_capacity(2 * hdim);
        tok.extend_from_slice(h);
        tok.extend(self.text_emb.row(self.cfg.sgs as usize).iter().copied());
        // ort's `(shape, Vec)` constructor rejects a 0-sized dimension, so build
        // the empty KV cache via the allocator (which keeps the shape verbatim),
        // matching the reference's `np.zeros((1,H,0,hd))`.
        let alloc = ort::memory::Allocator::default();
        let empty_k = ort::value::Tensor::<f32>::new(&alloc, [1usize, loc_heads, 0, loc_hd])
            .map_err(|e| e.to_string())?;
        let empty_v = ort::value::Tensor::<f32>::new(&alloc, [1usize, loc_heads, 0, loc_hd])
            .map_err(|e| e.to_string())?;
        let seed_inputs = ort::inputs![
            "token_emb" => Value::from_array((vec![1usize, 2, hdim], tok)).map_err(|e| e.to_string())?,
            "position_ids" => Value::from_array((vec![1usize, 2], vec![0i64, 1])).map_err(|e| e.to_string())?,
            "past_k_0" => empty_k,
            "past_v_0" => empty_v,
        ];
        let out = self
            .sess_ac
            .run(seed_inputs)
            .map_err(|e| format!("acoustic seed run: {}", e))?;
        let hidden = extract_f32(&out, "hidden")?; // (1,2,H) → 2*H
        let mut pk = extract_f32(&out, "present_k_0")?;
        let mut pv = extract_f32(&out, "present_v_0")?;
        // slot0 = hidden[0,0] (EOS probe); codebook-0 uses hidden[0,1].
        let slot0 =
            checked_tensor_window(&hidden, 0, hdim, "acoustic seed hidden slot 0")?.to_vec();
        let mut past_len = 2usize;
        drop(out);

        let mut codes: Vec<i64> = Vec::with_capacity(self.cfg.n_vq);
        let codebook_zero_hidden =
            checked_tensor_window(&hidden, hdim, hdim, "acoustic seed hidden slot 1")?;
        let c0 = self.sample_codebook(0, codebook_zero_hidden, &mut hist[0]);
        codes.push(c0);

        // Cùng lý do với vòng decode chính: past_len là độ dài KV-cache của
        // head acoustic (khởi đầu 2 = [hidden, c0]), không phải biến đếm.
        #[allow(clippy::explicit_counter_loop)]
        for ch in 1..self.cfg.n_vq {
            // token_emb = audio_emb[ch-1][codes[ch-1]]
            let prev_emb: Vec<f32> = self
                .audio_emb
                .index_axis(Axis(0), ch - 1)
                .index_axis(Axis(0), codes[ch - 1] as usize)
                .iter()
                .copied()
                .collect();
            let step_inputs = ort::inputs![
                "token_emb" => Value::from_array((vec![1usize, 1, hdim], prev_emb)).map_err(|e| e.to_string())?,
                "position_ids" => Value::from_array((vec![1usize, 1], vec![(ch + 1) as i64])).map_err(|e| e.to_string())?,
                "past_k_0" => Value::from_array((vec![1usize, loc_heads, past_len, loc_hd], pk.clone())).map_err(|e| e.to_string())?,
                "past_v_0" => Value::from_array((vec![1usize, loc_heads, past_len, loc_hd], pv.clone())).map_err(|e| e.to_string())?,
            ];
            let out = self
                .sess_ac
                .run(step_inputs)
                .map_err(|e| format!("acoustic step run: {}", e))?;
            let hd = extract_f32(&out, "hidden")?; // (1,1,H)
            pk = extract_f32(&out, "present_k_0")?;
            pv = extract_f32(&out, "present_v_0")?;
            let hvec = checked_tensor_window(&hd, 0, hdim, "acoustic step hidden")?.to_vec();
            drop(out);
            let code = self.sample_codebook(ch, &hvec, &mut hist[ch]);
            codes.push(code);
            past_len += 1;
        }

        // EOS when argmax(slot0 · text_emb^T) == speech_generation_end.
        let slot0v = ArrayView1::from(&slot0);
        let text_logits = self.text_emb.dot(&slot0v);
        let eos = argmax(text_logits.as_slice().unwrap()) as i64 == self.cfg.eos_speech;
        Ok((codes, eos))
    }

    /// Sample one code for codebook `ch` from `logits = vec · audio_emb[ch]^T`.
    fn sample_codebook(
        &mut self,
        ch: usize,
        vec: &[f32],
        seen: &mut std::collections::HashSet<i64>,
    ) -> i64 {
        let v = ArrayView1::from(vec);
        let mut logits = self.audio_emb.index_axis(Axis(0), ch).dot(&v).to_vec();
        let code = sample(&mut logits, seen, &mut self.rng);
        seen.insert(code);
        code
    }

    /// Decode all frames (T, n_vq) to a mono 48 kHz waveform via the MOSS codec.
    fn decode_codes(&mut self, frames: &[Vec<i64>]) -> Result<Vec<f32>, String> {
        let t = frames.len();
        let n = self.cfg.n_vq;
        let mut codes = Vec::with_capacity(t * n);
        for f in frames {
            for i in 0..n {
                codes.push(*f.get(i).unwrap_or(&0) as i32);
            }
        }
        let inputs = ort::inputs![
            "audio_codes" => Value::from_array((vec![1usize, t, n], codes)).map_err(|e| e.to_string())?,
            "audio_code_lengths" => Value::from_array((vec![1usize], vec![t as i32])).map_err(|e| e.to_string())?,
        ];
        let out = self
            .sess_codec
            .run(inputs)
            .map_err(|e| format!("codec decode run: {}", e))?;
        // audio: (1, channels, samples) → mean over channels → mono.
        let audio_val = out.get("audio").ok_or("codec missing 'audio' output")?;
        let (shape, data) = audio_val
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        if shape.len() != 3 {
            return Err(format!("unexpected codec audio shape {:?}", shape));
        }
        let channels = usize::try_from(shape[1])
            .map_err(|_| format!("codec returned negative channel count: {}", shape[1]))?;
        let samples = usize::try_from(shape[2])
            .map_err(|_| format!("codec returned negative sample count: {}", shape[2]))?;
        if channels == 0 {
            return Err("codec returned zero audio channels".to_string());
        }
        let expected_values = channels
            .checked_mul(samples)
            .ok_or_else(|| "codec audio tensor size overflow".to_string())?;
        if data.len() != expected_values {
            return Err(format!(
                "codec audio tensor length mismatch: shape {:?} requires {expected_values} values, got {}",
                shape,
                data.len()
            ));
        }
        let mut mono = vec![0.0f32; samples];
        for c in 0..channels {
            let base = c * samples;
            let channel = checked_tensor_window(data, base, samples, "codec audio channel")?;
            for (sample, value) in mono.iter_mut().zip(channel) {
                *sample += value;
            }
        }
        if channels > 1 {
            let inv = 1.0 / channels as f32;
            for v in mono.iter_mut() {
                *v *= inv;
            }
        }
        Ok(mono)
    }
}

/// Extract an output tensor to an owned `Vec<f32>` (used for KV-cache feedback).
fn extract_f32(outputs: &ort::session::SessionOutputs, name: &str) -> Result<Vec<f32>, String> {
    let val = outputs
        .get(name)
        .ok_or_else(|| format!("missing output '{}'", name))?;
    let (_, data) = val
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("extract '{}': {}", name, e))?;
    Ok(data.to_vec())
}

fn checked_embedding_index(id: i64, upper_bound: usize, label: &str) -> Result<usize, String> {
    let index =
        usize::try_from(id).map_err(|_| format!("{label} id must not be negative: {id}"))?;
    if index >= upper_bound {
        return Err(format!(
            "{label} id {id} is outside embedding vocabulary of size {upper_bound}"
        ));
    }
    Ok(index)
}

fn checked_tensor_window<'a>(
    values: &'a [f32],
    start: usize,
    len: usize,
    label: &str,
) -> Result<&'a [f32], String> {
    let end = start
        .checked_add(len)
        .ok_or_else(|| format!("{label} range overflows address space"))?;
    values.get(start..end).ok_or_else(|| {
        format!(
            "{label} tensor is too short: need range {start}..{end}, got {} values",
            values.len()
        )
    })
}

fn argmax(v: &[f32]) -> usize {
    let mut best = 0usize;
    let mut best_v = f32::NEG_INFINITY;
    for (i, &x) in v.iter().enumerate() {
        if x > best_v {
            best_v = x;
            best = i;
        }
    }
    best
}

fn softmax(logits: &[f32]) -> Vec<f32> {
    let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let mut exps: Vec<f32> = logits.iter().map(|&x| (x - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    if sum > 0.0 {
        for e in exps.iter_mut() {
            *e /= sum;
        }
    }
    exps
}

/// Sample an index from `logits` with repetition penalty → temperature →
/// top-k → top-p → multinomial (mirror `_sample`).
fn sample(logits: &mut [f32], prev: &std::collections::HashSet<i64>, rng: &mut StdRng) -> i64 {
    if (REP_PEN - 1.0).abs() > f32::EPSILON && !prev.is_empty() {
        for &idx in prev.iter() {
            let i = idx as usize;
            if i < logits.len() {
                let l = logits[i];
                logits[i] = if l < 0.0 { l * REP_PEN } else { l / REP_PEN };
            }
        }
    }
    if TEMPERATURE <= 0.0 {
        return argmax(logits) as i64;
    }
    for l in logits.iter_mut() {
        *l /= TEMPERATURE;
    }
    // top-k: keep the k largest, others → -inf.
    if TOP_K > 0 && TOP_K < logits.len() {
        let mut sorted: Vec<f32> = logits.to_vec();
        sorted.sort_unstable_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        let kth = sorted[TOP_K - 1];
        for l in logits.iter_mut() {
            if *l < kth {
                *l = f32::NEG_INFINITY;
            }
        }
    }
    // top-p (nucleus): drop the tail whose exclusive cumulative prob exceeds p.
    if TOP_P < 1.0 {
        let mut idx: Vec<usize> = (0..logits.len()).collect();
        idx.sort_unstable_by(|&a, &b| {
            logits[b]
                .partial_cmp(&logits[a])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let ordered: Vec<f32> = idx.iter().map(|&i| logits[i]).collect();
        let probs = softmax(&ordered);
        let mut cum = 0.0f32;
        for (rank, &i) in idx.iter().enumerate() {
            let excl = cum;
            cum += probs[rank];
            if excl > TOP_P {
                logits[i] = f32::NEG_INFINITY;
            }
        }
    }
    let probs = softmax(logits);
    let r: f32 = rng.r#gen::<f32>();
    let mut acc = 0.0f32;
    for (i, &p) in probs.iter().enumerate() {
        acc += p;
        if r < acc {
            return i as i64;
        }
    }
    (probs.len() - 1) as i64
}

/// 192-d speaker embedding → (H,) anchor: Linear (xvec_w·x + b) then LayerNorm.
fn speaker_anchor(
    speaker_emb: &[f32],
    xvec_w: &Array2<f32>,
    xvec_b: &Array1<f32>,
    ln_w: &Array1<f32>,
    ln_b: &Array1<f32>,
    eps: f32,
) -> Result<Array1<f32>, String> {
    if speaker_emb.is_empty() {
        return Err("speaker_emb is empty".to_string());
    }
    if speaker_emb.iter().any(|value| !value.is_finite()) {
        return Err("speaker_emb contains a non-finite value".to_string());
    }
    if speaker_emb.iter().all(|&x| x == 0.0) {
        return Err("speaker_emb is all-zero — not a valid anchor".to_string());
    }
    let x = ArrayView1::from(speaker_emb);
    if xvec_w.shape()[1] != x.len() {
        return Err(format!(
            "xvec_w cols {} != speaker_emb len {}",
            xvec_w.shape()[1],
            x.len()
        ));
    }
    let projected_size = xvec_w.shape()[0];
    for (name, actual) in [
        ("xvec_b", xvec_b.len()),
        ("xvec_ln_w", ln_w.len()),
        ("xvec_ln_b", ln_b.len()),
    ] {
        if actual != projected_size {
            return Err(format!(
                "{name} length {actual} does not match projection size {projected_size}"
            ));
        }
    }
    if projected_size == 0 {
        return Err("xvec projection has zero output dimensions".to_string());
    }
    if !eps.is_finite() || eps <= 0.0 {
        return Err("xvec LayerNorm epsilon must be finite and positive".to_string());
    }
    if xvec_w.iter().any(|value| !value.is_finite())
        || xvec_b.iter().any(|value| !value.is_finite())
        || ln_w.iter().any(|value| !value.is_finite())
        || ln_b.iter().any(|value| !value.is_finite())
    {
        return Err("xvec projection contains a non-finite value".to_string());
    }
    let mut v = xvec_w.dot(&x) + xvec_b; // (H,)
    let mean = v.mean().unwrap_or(0.0);
    let var = v.iter().map(|&z| (z - mean) * (z - mean)).sum::<f32>() / v.len() as f32;
    let denom = (var + eps).sqrt();
    v.mapv_inplace(|z| (z - mean) / denom);
    let anchor = &v * ln_w + ln_b;
    if anchor.iter().any(|value| !value.is_finite()) {
        return Err("speaker anchor contains a non-finite value".to_string());
    }
    Ok(anchor)
}

/// Pick a preset voice → (name, speaker_emb, ref_codes, style_id).
/// Kết quả chọn preset giọng: (tên, style vector, ref codes, ref_len) — đúng
/// bốn thứ `VieNeuVoice::load` cần để dựng prompt tham chiếu.
type SelectedVoice = (String, Vec<f32>, Vec<Vec<i64>>, i64);

fn select_voice(
    voices: &serde_json::Value,
    cfg_json: &serde_json::Value,
    wanted: Option<&str>,
) -> Result<SelectedVoice, String> {
    let presets = voices
        .get("presets")
        .and_then(|p| p.as_object())
        .ok_or("voices json has no 'presets' object")?;
    let default_name = voices
        .get("default_voice")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let name = match wanted {
        Some(w) if presets.contains_key(w) => w.to_string(),
        Some(w) => return Err(format!("voice '{}' not found in presets", w)),
        None if presets.contains_key(default_name) => default_name.to_string(),
        None => presets
            .keys()
            .next()
            .cloned()
            .ok_or("no preset voices available")?,
    };
    let entry = &presets[&name];

    let speaker_emb: Vec<f32> = entry
        .get("speaker_emb")
        .and_then(|v| v.as_array())
        .ok_or("preset missing 'speaker_emb'")?
        .iter()
        .map(|x| x.as_f64().unwrap_or(0.0) as f32)
        .collect();

    let ref_codes: Vec<Vec<i64>> = entry
        .get("codes")
        .and_then(|v| v.as_array())
        .map(|frames| {
            frames
                .iter()
                .filter_map(|f| f.as_array())
                .map(|f| f.iter().map(|x| x.as_i64().unwrap_or(0)).collect())
                .collect()
        })
        .unwrap_or_default();

    let style_str = entry.get("style").and_then(|v| v.as_str()).unwrap_or("");
    let default_style = cfg_json
        .get("default_style_token_id")
        .and_then(|v| v.as_i64())
        .unwrap_or(16);
    let style_id = cfg_json
        .get("style_labels")
        .and_then(|m| m.get(style_str))
        .and_then(|v| v.as_i64())
        .unwrap_or(default_style);

    Ok((name, speaker_emb, ref_codes, style_id))
}

#[cfg(test)]
mod config_validation_tests {
    use super::{Cfg, checked_embedding_index, checked_tensor_window, speaker_anchor};
    use ndarray::{Array1, array};

    fn valid_config() -> serde_json::Value {
        serde_json::json!({
            "n_vq": 16,
            "hidden_size": 1024,
            "num_hidden_layers": 24,
            "num_key_value_heads": 8,
            "head_dim": 128,
            "local_num_attention_heads": 8,
            "audio_pad_token_id": 0,
            "text_prompt_start_token_id": 1,
            "text_prompt_end_token_id": 2,
            "speech_generation_start_token_id": 3,
            "speech_generation_end_token_id": 4,
            "audio_ref_slot_token_id": 5
        })
    }

    #[test]
    fn invalid_dimensions_return_error_instead_of_panicking() {
        let mut zero_heads = valid_config();
        zero_heads["local_num_attention_heads"] = serde_json::json!(0);
        let outcome = std::panic::catch_unwind(|| Cfg::from_json(&zero_heads));
        assert!(outcome.is_ok(), "zero attention heads must not panic");
        assert!(outcome.unwrap().is_err());

        let mut negative_dimension = valid_config();
        negative_dimension["hidden_size"] = serde_json::json!(-1);
        assert!(Cfg::from_json(&negative_dimension).is_err());

        let mut uneven_heads = valid_config();
        uneven_heads["hidden_size"] = serde_json::json!(1025);
        assert!(Cfg::from_json(&uneven_heads).is_err());

        let mut negative_token = valid_config();
        negative_token["speech_generation_start_token_id"] = serde_json::json!(-1);
        assert!(Cfg::from_json(&negative_token).is_err());
    }

    #[test]
    fn embedding_ids_outside_vocab_are_rejected() {
        assert_eq!(checked_embedding_index(0, 2, "text").unwrap(), 0);
        assert_eq!(checked_embedding_index(1, 2, "text").unwrap(), 1);
        assert!(checked_embedding_index(-1, 2, "text").is_err());
        assert!(checked_embedding_index(2, 2, "text").is_err());
        assert!(checked_embedding_index(0, 0, "text").is_err());
    }

    #[test]
    fn tensor_windows_reject_truncated_or_overflowing_outputs() {
        let values = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(
            checked_tensor_window(&values, 1, 2, "hidden").unwrap(),
            &[2.0, 3.0]
        );
        assert!(checked_tensor_window(&values, 3, 2, "hidden").is_err());
        assert!(checked_tensor_window(&values, usize::MAX, 2, "hidden").is_err());
    }

    #[test]
    fn speaker_anchor_rejects_incompatible_projection_shapes() {
        let speaker = [1.0, 2.0];
        let weights = array![[1.0, 0.0], [0.0, 1.0]];
        let valid = Array1::from_vec(vec![0.0, 0.0]);
        let short = Array1::from_vec(vec![0.0]);

        let outcome = std::panic::catch_unwind(|| {
            speaker_anchor(&speaker, &weights, &short, &valid, &valid, 1e-5)
        });
        assert!(outcome.is_ok(), "mismatched projection bias must not panic");
        assert!(outcome.unwrap().is_err());
        assert!(speaker_anchor(&speaker, &weights, &valid, &short, &valid, 1e-5).is_err());
        assert!(speaker_anchor(&speaker, &weights, &valid, &valid, &short, 1e-5).is_err());
        assert!(speaker_anchor(&speaker, &weights, &valid, &valid, &valid, -1.0).is_err());
    }
}
