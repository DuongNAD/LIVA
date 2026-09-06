use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::{KvCacheType, LlamaContextParams, LlamaPoolingType};
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::AddBos;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::mtmd::{
    MtmdBitmap, MtmdContext, MtmdContextParams, MtmdInputText, mtmd_default_marker,
};
use llama_cpp_2::token::LlamaToken;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Lỗi khi CHƯA có model nào được nạp.
///
/// Tách thành hằng số vì nó **không phải lỗi** theo nghĩa người dùng: router
/// được nạp bất đồng bộ lúc boot (`boot.rs`), nên mọi câu hỏi tới trong cửa sổ
/// đó đều rơi vào đây. Nơi hiển thị **phải phân biệt được** nó với hỏng thật —
/// gộp chung sẽ nói với người dùng rằng hệ thống lỗi trong khi nó chỉ đang khởi
/// động, và đó đúng là chuyện đã xảy ra ngày 02/08/2026 sau khi đổi router sang
/// một model 4,2 GB.
///
/// So khớp bằng hằng số này, đừng gõ lại chuỗi: nơi sinh và nơi phân loại mà
/// trôi khỏi nhau thì thông điệp lại âm thầm sai như cũ.
pub const ERR_NO_MODEL: &str = "No model loaded";

/// Image input for [`LlamaRouterManager::answer_with_image`].
pub enum VisionImage<'a> {
    /// Raw RGB pixels (`width*height*3` bytes), e.g. from a screen capture.
    Rgb {
        width: u32,
        height: u32,
        data: &'a [u8],
    },
    /// Encoded file bytes (PNG/JPG/BMP…), decoded by llama.cpp's stb_image.
    Encoded(&'a [u8]),
}

static GLOBAL_BACKEND: OnceLock<LlamaBackend> = OnceLock::new();

pub fn get_backend() -> &'static LlamaBackend {
    GLOBAL_BACKEND
        .get_or_init(|| LlamaBackend::init().expect("Failed to initialize llama.cpp backend"))
}

pub struct LlamaEngine {
    // Declaring context (and the multimodal ctx) before model ensures they are
    // dropped first, preventing dangling references into the model.
    pub context: LlamaContext<'static>,
    /// Multimodal (vision) context, built lazily from the configured mmproj on
    /// the first `answer_with_image` call. `None` for text-only models.
    pub mtmd: Option<MtmdContext>,
    pub model: LlamaModel,
}

unsafe impl Send for LlamaEngine {}
unsafe impl Sync for LlamaEngine {}

#[derive(Debug, Clone)]
pub struct CompletionOutput {
    pub text: String,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
}

pub struct LlamaRouterManager {
    pub engine: Option<LlamaEngine>,
    pub n_ctx: usize,
    pub n_gpu_layers: u32,
    pub current_model_path: PathBuf,
    pub last_tokens: Vec<LlamaToken>,
    pub vocab_only: bool,
    /// Path to the vision projector (mmproj) GGUF for multimodal models; the
    /// `MtmdContext` is built lazily from it on the first vision request.
    pub mmproj_path: Option<PathBuf>,
}

unsafe impl Send for LlamaRouterManager {}
unsafe impl Sync for LlamaRouterManager {}

/// Số token chừa lại cho phần model sinh ra. Prompt cộng với ngần này phải nhỏ
/// hơn `n_ctx`, nếu không `generate_completion` từ chối ngay thay vì để
/// `decode()` hỏng giữa chừng.
pub const RESERVE_FOR_COMPLETION: usize = 512;

/// Kiểm tra prompt có lọt `n_ctx` không, chừa chỗ cho phần trả lời.
///
/// Tách riêng khỏi `generate_completion` để test được mà không cần nạp model:
/// bản thân `generate_completion` thoát sớm với model vocab-only nên guard
/// nằm trong đó sẽ không bao giờ được test chạm tới.
///
/// `n_ctx` nhỏ hơn hoặc bằng `RESERVE_FOR_COMPLETION` thì mọi prompt đều bị từ
/// chối — đúng ý đồ: context nhỏ như vậy không sinh nổi câu trả lời tử tế.
pub fn check_prompt_fits(prompt_tokens_len: usize, n_ctx: usize) -> Result<(), String> {
    // saturating_add: cộng thẳng sẽ tràn và panic ở debug build khi
    // prompt_tokens_len gần usize::MAX.
    if prompt_tokens_len.saturating_add(RESERVE_FOR_COMPLETION) < n_ctx {
        return Ok(());
    }
    Err(format!(
        "Prompt qua dai: {} token, n_ctx = {} (can chua {} token cho phan tra loi). \
         Hay cat bot lich su hoi thoai (LIVA_MAX_HISTORY_MESSAGES) hoac tang LIVA_LLM_N_CTX.",
        prompt_tokens_len, n_ctx, RESERVE_FOR_COMPLETION
    ))
}

/// Compute the length of the common token prefix between two token sequences.
#[inline]
pub fn compute_common_prefix_len(a: &[LlamaToken], b: &[LlamaToken]) -> usize {
    a.iter()
        .zip(b.iter())
        .take_while(|(t1, t2)| t1 == t2)
        .count()
}

pub fn prune_kv_cache(
    context: &mut LlamaContext,
    n_past: &mut i32,
    n_ctx: i32,
    last_tokens: &mut Vec<LlamaToken>,
) {
    if n_ctx <= 0 {
        return;
    }
    let s = (n_ctx / 8).clamp(1, 512); // Keep early conversation tokens
    let k = (n_ctx / 8).clamp(1, 512); // Discard chunk size

    if *n_past >= n_ctx && (s + k) <= n_ctx {
        // Discard sequence entries
        let _ = context.clear_kv_cache_seq(Some(0), Some(s as u32), Some((s + k) as u32));
        // Shift remaining token sequence indices
        let _ = context.kv_cache_seq_add(0, Some((s + k) as u32), Some(*n_past as u32), -k);
        *n_past -= k;
        let start = s as usize;
        let end = (s + k) as usize;
        if last_tokens.len() >= end {
            last_tokens.drain(start..end);
        }
    }
}

impl LlamaRouterManager {
    pub fn new(n_ctx: usize, n_gpu_layers: u32) -> Result<Self, String> {
        let _ = get_backend();
        Ok(Self {
            engine: None,
            n_ctx,
            n_gpu_layers,
            current_model_path: PathBuf::new(),
            last_tokens: Vec::new(),
            vocab_only: false,
            mmproj_path: None,
        })
    }

    /// Returns the currently cached token sequence.
    pub fn last_tokens(&self) -> &[LlamaToken] {
        &self.last_tokens
    }

    /// Explicitly clear the KV cache and reset the cached token sequence.
    pub fn clear_kv_cache(&mut self) {
        if let Some(engine) = self.engine.as_mut() {
            engine.context.clear_kv_cache();
        }
        self.last_tokens.clear();
    }

    /// Calculates how many tokens of `prompt_tokens` match the currently cached prefix.
    pub fn prefix_cached_len(&self, prompt_tokens: &[LlamaToken]) -> usize {
        compute_common_prefix_len(&self.last_tokens, prompt_tokens)
    }

    /// Set the vision-projector (mmproj) GGUF path used to lazily build the
    /// multimodal context for `answer_with_image`. Call before/after `swap_model`;
    /// the existing engine's cached `MtmdContext` (if any) is invalidated so the
    /// next vision request rebuilds it against the new projector.
    pub fn set_mmproj_path(&mut self, path: Option<PathBuf>) {
        if self.mmproj_path != path {
            self.mmproj_path = path;
            if let Some(engine) = self.engine.as_mut() {
                engine.mtmd = None;
            }
        }
    }

    pub async fn swap_model(
        &mut self,
        new_model_path: &Path,
        n_ctx: Option<usize>,
        n_gpu_layers: Option<u32>,
        vocab_only: Option<bool>,
    ) -> Result<(), String> {
        // 1. Release active model/context to drop VRAM instantly
        if self.engine.is_some() {
            self.engine = None;
            self.last_tokens.clear();
        }

        // 2. Yield to Tokio to let GPU drivers settle VRAM allocations
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // 3. Load parameters (enforce use_mmap=true, use_mlock=false)
        let target_n_gpu_layers = n_gpu_layers.unwrap_or(self.n_gpu_layers);
        let target_n_ctx = n_ctx.unwrap_or(self.n_ctx);
        let target_vocab_only = vocab_only.unwrap_or(false);

        let mut model_params = LlamaModelParams::default();
        model_params = model_params.with_n_gpu_layers(target_n_gpu_layers);
        model_params = model_params.with_use_mmap(true);
        model_params = model_params.with_use_mlock(false);
        model_params = model_params.with_vocab_only(target_vocab_only);

        // 4. Load weights
        let backend = get_backend();
        let model = LlamaModel::load_from_file(backend, new_model_path, &model_params)
            .map_err(|e| format!("Failed to load GGUF file: {:?}", e))?;

        // Detect the model family from its embedded chat template so the prompt
        // compiler emits the format the model was trained on: ChatML
        // (`<|im_start|>`, Qwen3-VL), gemma-4 (`<|turn>`), or classic gemma
        // (`<start_of_turn>`).
        let chat_template = model
            .meta_val_str("tokenizer.chat_template")
            .unwrap_or_default();
        let is_chatml = chat_template.contains("<|im_start|>");
        let gemma4_markers = !is_chatml && chat_template.contains("<|turn>");
        super::prompt::CHATML.store(is_chatml, std::sync::atomic::Ordering::Relaxed);
        super::prompt::GEMMA4_MARKERS.store(gemma4_markers, std::sync::atomic::Ordering::Relaxed);
        tracing::info!(
            "Prompt format: {}",
            if is_chatml {
                "ChatML (<|im_start|>) — Qwen-style"
            } else if gemma4_markers {
                "gemma-4 (<|turn>)"
            } else {
                "classic gemma (<start_of_turn>)"
            }
        );

        // 5. Setup context with Q8 KV Cache compression (type_k = 8, type_v = 8)
        let threads = std::env::var("LIVA_LLM_THREADS")
            .unwrap_or_else(|_| "4".to_string())
            .parse::<i32>()
            .unwrap_or(4);

        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(
                std::num::NonZeroU32::new(target_n_ctx as u32).ok_or("Invalid n_ctx")?,
            ))
            .with_embeddings(true)
            .with_pooling_type(LlamaPoolingType::Mean)
            .with_type_k(KvCacheType::Q8_0)
            .with_type_v(KvCacheType::Q8_0)
            .with_n_threads(threads)
            .with_n_threads_batch(threads);

        let context = model
            .new_context(backend, ctx_params)
            .map_err(|e| format!("Failed to create llama context: {:?}", e))?;

        // Safely transmute LlamaContext to have 'static lifetime
        let context_static =
            unsafe { std::mem::transmute::<LlamaContext<'_>, LlamaContext<'static>>(context) };

        self.engine = Some(LlamaEngine {
            context: context_static,
            mtmd: None,
            model,
        });
        self.n_ctx = target_n_ctx;
        self.n_gpu_layers = target_n_gpu_layers;
        self.current_model_path = new_model_path.to_path_buf();
        self.vocab_only = target_vocab_only;

        Ok(())
    }

    /// Generate one assistant response.
    ///
    /// Internal reasoning/control channels are removed before both the
    /// returned text and `token_callback`. While reasoning is hidden, the
    /// callback receives an empty heartbeat so cancellation-aware callers can
    /// still stop generation at the next token boundary.
    pub fn generate_completion<F>(
        &mut self,
        prompt: &str,
        temperature: f32,
        top_p: f32,
        mut token_callback: F,
    ) -> Result<CompletionOutput, String>
    where
        F: FnMut(&str) -> bool,
    {
        if self.vocab_only {
            return Err("Cannot generate completions on a vocab-only model".to_string());
        }

        let engine = self.engine.as_mut().ok_or(ERR_NO_MODEL)?;

        // Tokenize prompt
        let prompt_tokens = engine
            .model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| format!("StringToTokenError: {:?}", e))?;
        let prompt_tokens_len = prompt_tokens.len();

        // Lớp 2 của F2 — guard cứng. Prefill ở bước 3 nạp toàn bộ tail_tokens
        // vào MỘT batch rồi mới tới vòng lặp có prune_kv_cache, nên prompt dài
        // hơn n_ctx làm `decode()` hỏng với thông báo khó hiểu ("Decode failed").
        // Guard này biến nó thành lỗi đọc được, và chặn cho MỌI caller —
        // chat:completion, task_plan_chat, vision:ask, agent graph, Telegram —
        // kể cả những chỗ quên cắt lịch sử.
        check_prompt_fits(prompt_tokens_len, self.n_ctx)?;

        // 1. Find common prefix with last processed tokens
        let common_len = compute_common_prefix_len(&self.last_tokens, &prompt_tokens);

        // 2. Clear KV cache after common prefix
        if common_len > 0 {
            if common_len < self.last_tokens.len() {
                let _ = engine
                    .context
                    .clear_kv_cache_seq(Some(0), Some(common_len as u32), None);
                self.last_tokens.truncate(common_len);
            }
        } else {
            engine.context.clear_kv_cache();
            self.last_tokens.clear();
        }

        let mut n_past = common_len as i32;
        let tail_tokens = &prompt_tokens[common_len..];

        // 3. Prefill tail tokens
        if !tail_tokens.is_empty() {
            let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(tail_tokens.len(), 1);
            for (i, token) in tail_tokens.iter().enumerate() {
                let pos = n_past + i as i32;
                let is_last = i == tail_tokens.len() - 1;
                batch
                    .add(*token, pos, &[0], is_last)
                    .map_err(|e| format!("Failed to add token: {:?}", e))?;
            }
            engine
                .context
                .decode(&mut batch)
                .map_err(|e| format!("Decode failed: {:?}", e))?;
            n_past += tail_tokens.len() as i32;
        }

        self.last_tokens = prompt_tokens;

        let mut sampler = super::sampler::create_sampler(temperature, top_p);
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut response_text = String::new();
        let mut raw_response_bytes = 0usize;
        let mut output_filter = super::output_filter::VisibleOutputFilter::from_prompt_tail(prompt);
        let mut callback_active = true;
        let mut completion_tokens_count = 0;

        // 4. Token generation loop
        loop {
            prune_kv_cache(
                &mut engine.context,
                &mut n_past,
                self.n_ctx as i32,
                &mut self.last_tokens,
            );

            // -1 = "last logits row" (canonical llama.cpp usage). Index 0 is
            // only coincidentally correct for single-token batches and reads
            // the wrong row right after a multi-token prefill.
            let token = sampler.sample(&engine.context, -1);
            sampler.accept(token);
            completion_tokens_count += 1;

            // is_eog_token covers <eos> plus chat-turn terminators like
            // Gemma's <end_of_turn>; matching only token_eos() lets chat
            // models generate past their turn until the safety valve.
            if engine.model.is_eog_token(token) {
                break;
            }

            if let Ok(piece) = engine
                .model
                .token_to_piece(token, &mut decoder, false, None)
                && !piece.is_empty()
            {
                raw_response_bytes = raw_response_bytes.saturating_add(piece.len());
                let keep_running = super::output_filter::forward_filtered_piece(
                    &mut output_filter,
                    &piece,
                    &mut |visible| {
                        response_text.push_str(visible);
                        token_callback(visible)
                    },
                );
                if !keep_running {
                    callback_active = false;
                    break;
                }
            }

            self.last_tokens.push(token);

            let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(1, 1);
            batch
                .add(token, n_past, &[0], true)
                .map_err(|e| format!("Failed to add next token: {:?}", e))?;

            engine
                .context
                .decode(&mut batch)
                .map_err(|e| format!("Decode failed on next token: {:?}", e))?;

            n_past += 1;

            if raw_response_bytes > 100_000 || self.last_tokens.len() > self.n_ctx * 2 {
                break;
            }
        }

        let tail = output_filter.finish();
        response_text.push_str(&tail);
        if callback_active && !tail.is_empty() {
            let _ = token_callback(&tail);
        }

        Ok(CompletionOutput {
            text: response_text,
            prompt_tokens: prompt_tokens_len,
            completion_tokens: completion_tokens_count,
        })
    }

    /// Answer a question about an image using the multimodal (vision) path of a
    /// VL model (e.g. Qwen3-VL). The `MtmdContext` is built lazily from
    /// `mmproj_path` on first use. Streams pieces to `token_callback` (return
    /// false to stop).
    ///
    /// NOTE: on Windows this requires a **release** build — see the CRT guard
    /// at the top of the body. (The doc link here used to point at
    /// `quiet_crt_assert`, a function that has never existed; rustdoc would
    /// flag it, but `cargo doc` is not a CI step so nothing did.)
    ///
    /// Measured 2026-07-26 on release, Qwen3-VL-2B + mmproj-F16, CPU only:
    /// **~80 s per call** and flat across calls (80.2 / 81.3 / 79.0 s), i.e.
    /// this is per-image cost, not first-call warmup. llama.cpp attributes it
    /// to ~56 s encoding the image slice plus ~29 s decoding 2040 image tokens
    /// in four batches. A CUDA build is the obvious untested lever.
    pub fn answer_with_image<F>(
        &mut self,
        question: &str,
        image: VisionImage,
        temperature: f32,
        top_p: f32,
        mut token_callback: F,
    ) -> Result<CompletionOutput, String>
    where
        F: FnMut(&str) -> bool,
    {
        if self.vocab_only {
            return Err("Cannot generate on a vocab-only model".to_string());
        }
        // Vision needs a RELEASE build on Windows: the debug CRT asserts in the
        // clip/mmproj file loader (llama.cpp links the debug CRT, Rust the release
        // one → fd-table mismatch) and aborts the process. Return a clean error
        // instead of crashing (callers surface it as a spoken/UI apology).
        if cfg!(all(windows, debug_assertions)) {
            return Err(
                "Vision requires a release build (debug CRT assertion in the mmproj loader) — \
                 run the core with `cargo build --release`."
                    .to_string(),
            );
        }
        let n_gpu_layers = self.n_gpu_layers;
        let mmproj_path = self
            .mmproj_path
            .clone()
            .ok_or("No mmproj (vision projector) configured")?;
        // A vision turn always starts a fresh sequence, so drop any KV-prefix
        // reuse state before borrowing the engine.
        self.last_tokens.clear();
        let engine = self.engine.as_mut().ok_or(ERR_NO_MODEL)?;

        // Lazily build the multimodal context from the projector.
        if engine.mtmd.is_none() {
            if !mmproj_path.exists() {
                return Err(format!("mmproj not found: {:?}", mmproj_path));
            }
            let threads = std::env::var("LIVA_LLM_THREADS")
                .ok()
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(4);
            let params = MtmdContextParams {
                use_gpu: n_gpu_layers > 0,
                print_timings: false,
                n_threads: threads,
                media_marker: CString::new(mtmd_default_marker()).map_err(|e| e.to_string())?,
                image_min_tokens: -1,
                image_max_tokens: -1,
            };
            let mtmd_path = mmproj_path.to_str().ok_or("mmproj path not UTF-8")?;
            let ctx = MtmdContext::init_from_file(mtmd_path, &engine.model, &params)
                .map_err(|e| format!("mtmd init from {:?}: {}", mmproj_path, e))?;
            engine.mtmd = Some(ctx);
        }
        engine.context.clear_kv_cache();

        // Disjoint field borrows: mtmd (&), context (&mut), model (&mut→&).
        let LlamaEngine {
            context,
            mtmd,
            model,
        } = engine;
        let mtmd = mtmd.as_ref().unwrap();

        let bitmap = match image {
            VisionImage::Rgb {
                width,
                height,
                data,
            } => MtmdBitmap::from_image_data(width, height, data).map_err(|e| e.to_string())?,
            VisionImage::Encoded(bytes) => {
                MtmdBitmap::from_buffer(mtmd, bytes, false).map_err(|e| e.to_string())?
            }
        };

        // Đi qua `compile_prompt` để prompt thị giác dùng ĐÚNG họ template mà
        // `swap_model` đã nhận diện, thay vì hard-code một họ.
        //
        // **Bug đã sửa 02/08/2026, tìm ra bằng `gemma4_probe`.** Bản cũ ghi cứng
        // ChatML (`<|im_start|>…<|im_end|>`) — đúng khi Qwen3-VL là model VL duy
        // nhất, sai ngay khi có model thứ hai. Với gemma-4 nó vẫn "chạy" (model
        // đủ khoẻ để trả lời), nên KHÔNG có lỗi nào được ném ra; triệu chứng duy
        // nhất là chuỗi `<|im_end|>` rò vào câu trả lời người dùng đọc được, vì
        // `VisibleOutputFilter` dựng theo đuôi prompt nên nó canh sai token dừng.
        // Đây đúng loại hỏng "trông vẫn ổn" mà chỉ một phép khẳng định tường
        // minh về template mới bắt được.
        //
        // Marker media để TRẦN: mtmd tự bọc nó bằng token thị giác của model —
        // đừng viết tay `<|vision_start|>…`.
        let prompt = super::prompt::compile_prompt(&[
            super::ChatMessage {
                role: "system".to_string(),
                content: super::persona::PERSONA_LIVA.to_string(),
            },
            super::ChatMessage {
                role: "user".to_string(),
                content: format!("{} {}", mtmd_default_marker(), question),
            },
        ])?;
        let mut output_filter =
            super::output_filter::VisibleOutputFilter::from_prompt_tail(&prompt);
        let input = MtmdInputText {
            text: prompt,
            add_special: true,
            parse_special: true,
        };
        let chunks = mtmd
            .tokenize(input, &[&bitmap])
            .map_err(|e| format!("mtmd tokenize: {}", e))?;
        let prompt_tokens = chunks.total_tokens();
        let mut n_past = chunks
            .eval_chunks(mtmd, &*context, 0, 0, 512, true)
            .map_err(|e| format!("mtmd eval: {}", e))?;

        let mut sampler = super::sampler::create_sampler(temperature, top_p);
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut text = String::new();
        let mut callback_active = true;
        let mut completion_tokens = 0usize;
        loop {
            let token = sampler.sample(&*context, -1);
            sampler.accept(token);
            completion_tokens += 1;
            if model.is_eog_token(token) {
                break;
            }
            if let Ok(piece) = model.token_to_piece(token, &mut decoder, false, None)
                && !piece.is_empty()
            {
                let keep_running = super::output_filter::forward_filtered_piece(
                    &mut output_filter,
                    &piece,
                    &mut |visible| {
                        text.push_str(visible);
                        token_callback(visible)
                    },
                );
                if !keep_running {
                    callback_active = false;
                    break;
                }
            }
            let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(1, 1);
            batch
                .add(token, n_past, &[0], true)
                .map_err(|e| format!("batch add: {:?}", e))?;
            context
                .decode(&mut batch)
                .map_err(|e| format!("decode: {:?}", e))?;
            n_past += 1;
            if completion_tokens >= 512 || text.len() > 100_000 {
                break;
            }
        }

        let tail = output_filter.finish();
        text.push_str(&tail);
        if callback_active && !tail.is_empty() {
            let _ = token_callback(&tail);
        }

        Ok(CompletionOutput {
            text,
            prompt_tokens,
            completion_tokens,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[tokio::test]
    async fn test_llama_router_manager_swap_and_tokenize() {
        let model_path = Path::new("../models/ggml-vocab-llama-spm.gguf");
        if !model_path.exists() {
            println!("Skipping test: model file not found");
            return;
        }

        let mut manager = LlamaRouterManager::new(128, 0).unwrap();

        let res = manager
            .swap_model(model_path, Some(128), Some(0), Some(true))
            .await;
        assert!(res.is_ok(), "Failed to swap model: {:?}", res.err());

        assert!(manager.engine.is_some());
        assert_eq!(manager.current_model_path, model_path);

        let engine = manager.engine.as_ref().unwrap();
        let tokens = engine.model.str_to_token("Hello world", AddBos::Always);
        assert!(tokens.is_ok());
        let tokens_val = tokens.unwrap();
        assert!(!tokens_val.is_empty());

        let completion_res = manager.generate_completion("Hello", 0.7, 0.9, |_| true);
        assert!(completion_res.is_err());
        assert_eq!(
            completion_res.err().unwrap(),
            "Cannot generate completions on a vocab-only model"
        );
    }

    #[tokio::test]
    async fn test_swap_model_detects_chatml_for_qwen() {
        // Auto-detection of the Qwen ChatML family from the GGUF chat_template.
        // vocab_only load is fast (metadata only) and enough to read the template.
        let model_path = std::path::PathBuf::from(
            "E:\\AI_Models\\Qwen3-VL-2B-Instruct-GGUF\\Qwen3-VL-2B-Instruct-Q4_K_M.gguf",
        );
        if !model_path.exists() {
            eprintln!("skipping: Qwen3-VL GGUF not on disk");
            return;
        }
        let mut manager = LlamaRouterManager::new(128, 0).unwrap();
        manager
            .swap_model(&model_path, Some(128), Some(0), Some(true))
            .await
            .expect("swap Qwen model (vocab_only)");
        assert!(
            super::super::prompt::CHATML.load(std::sync::atomic::Ordering::Relaxed),
            "swap_model should set CHATML=true for a Qwen (<|im_start|>) chat template"
        );
        assert!(
            !super::super::prompt::GEMMA4_MARKERS.load(std::sync::atomic::Ordering::Relaxed),
            "GEMMA4_MARKERS must be false for a ChatML model"
        );
    }

    #[test]
    fn test_llm_threads_env_parsing() {
        unsafe {
            std::env::set_var("LIVA_LLM_THREADS", "6");
        }

        let threads = std::env::var("LIVA_LLM_THREADS")
            .unwrap_or_else(|_| "4".to_string())
            .parse::<i32>()
            .unwrap_or(4);

        assert_eq!(threads, 6);

        unsafe {
            std::env::remove_var("LIVA_LLM_THREADS");
        }
    }

    #[test]
    fn guard_cho_qua_khi_prompt_du_ngan() {
        // 4096 - 512 = 3584 la nguong; 3583 phai lot
        assert!(check_prompt_fits(3583, 4096).is_ok());
        assert!(check_prompt_fits(0, 4096).is_ok());
        assert!(check_prompt_fits(100, 1024).is_ok());
    }

    #[test]
    fn guard_chan_dung_tai_nguong() {
        // dung bang nguong da phai chan: khong con cho cho phan tra loi
        let r = check_prompt_fits(3584, 4096);
        assert!(r.is_err(), "prompt_len + RESERVE == n_ctx phai bi chan");
        let msg = r.unwrap_err();
        assert!(msg.contains("3584"), "phai neu so token that: {}", msg);
        assert!(msg.contains("4096"), "phai neu n_ctx: {}", msg);
        assert!(
            msg.contains("LIVA_MAX_HISTORY_MESSAGES") && msg.contains("LIVA_LLM_N_CTX"),
            "phai chi ra cach khac phuc: {}",
            msg
        );
    }

    #[test]
    fn guard_chan_khi_prompt_dai_hon_ca_n_ctx() {
        assert!(check_prompt_fits(10_000, 4096).is_err());
    }

    #[test]
    fn guard_chan_moi_thu_khi_n_ctx_qua_nho() {
        // n_ctx <= RESERVE_FOR_COMPLETION: khong sinh noi cau tra loi tu te
        assert!(check_prompt_fits(0, RESERVE_FOR_COMPLETION).is_err());
        assert!(check_prompt_fits(0, 16).is_err());
        assert!(check_prompt_fits(1, 128).is_err());
    }

    #[test]
    fn guard_khong_tran_so_khi_prompt_cuc_lon() {
        // cong truc tiep se tran va panic o debug build
        assert!(check_prompt_fits(usize::MAX - 1, 4096).is_err());
        assert!(check_prompt_fits(usize::MAX, usize::MAX).is_err());
    }

    #[test]
    fn test_compute_common_prefix_len() {
        let t1 = LlamaToken(10);
        let t2 = LlamaToken(20);
        let t3 = LlamaToken(30);
        let t4 = LlamaToken(40);

        let seq_a = vec![t1, t2, t3];
        let seq_b = vec![t1, t2, t4];
        let seq_c = vec![t1, t2, t3, t4];
        let seq_d = vec![t4, t3];

        assert_eq!(compute_common_prefix_len(&seq_a, &seq_b), 2);
        assert_eq!(compute_common_prefix_len(&seq_a, &seq_c), 3);
        assert_eq!(compute_common_prefix_len(&seq_a, &seq_d), 0);
        assert_eq!(compute_common_prefix_len(&[], &seq_a), 0);
        assert_eq!(compute_common_prefix_len(&seq_a, &[]), 0);
        assert_eq!(compute_common_prefix_len(&seq_a, &seq_a), 3);
    }
}
