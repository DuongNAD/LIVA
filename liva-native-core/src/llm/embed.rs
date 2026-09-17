use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;

/// Kiểm tra số token đầu vào có vừa với `n_ctx` cho lượt tính embedding hay không.
///
/// ## Vì sao KHÔNG dùng lại `check_prompt_fits` (engine.rs:95)
///
/// `check_prompt_fits` trừ `RESERVE_FOR_COMPLETION` (512 token) vì đường sinh text
/// cần chừa chỗ cho câu trả lời của model. Đường embedding KHÔNG sinh thêm token nào
/// (chỉ một lượt forward pass trích xuất vector đặc trưng), nên có thể dùng trọn vẹn
/// tới `n_ctx`. Nếu dùng lại `check_prompt_fits`, các đoạn văn bản dài từ `n_ctx - 512`
/// tới `n_ctx` token sẽ bị từ chối oan dù hoàn toàn vừa với batch (`with_n_batch(target_n_ctx)`).
///
/// Nếu `tokens_len > n_ctx`, llama.cpp sẽ nổ `GGML_ASSERT(n_tokens_all <= cparams.n_batch)`
/// và gọi `abort()` làm chết cả tiến trình LIVA. Guard này chặn trước và trả Err rõ ràng.
pub fn check_embed_tokens_fit(tokens_len: usize, n_ctx: usize) -> Result<(), String> {
    if tokens_len <= n_ctx {
        return Ok(());
    }
    Err(format!(
        "Input qua dai cho embedding: {} token, n_ctx = {}. \
         Hay chia nho doan van ban truoc khi tao vector dac trung.",
        tokens_len, n_ctx
    ))
}

pub fn get_embedding(
    model: &LlamaModel,
    context: &mut LlamaContext,
    text: &str,
) -> Result<Vec<f32>, String> {
    context.clear_kv_cache();

    let tokens = model
        .str_to_token(text, llama_cpp_2::model::AddBos::Always)
        .map_err(|e| format!("Tokenization failed: {:?}", e))?;

    if tokens.is_empty() {
        return Ok(vec![0.0; model.n_embd() as usize]);
    }

    let n_ctx = context.n_ctx() as usize;
    check_embed_tokens_fit(tokens.len(), n_ctx)?;

    let mut batch = LlamaBatch::new(tokens.len(), 1);
    for (pos, token) in tokens.iter().enumerate() {
        // Mark logits=true for every token so mean pooling works correctly
        batch
            .add(*token, pos as i32, &[0], true)
            .map_err(|e| format!("Failed to add token to batch: {:?}", e))?;
    }

    context
        .decode(&mut batch)
        .map_err(|e| format!("Embedding decode failed: {:?}", e))?;

    let raw_emb = match context.embeddings_seq_ith(0) {
        Ok(emb) => emb,
        Err(_) => context
            .embeddings_ith((tokens.len() - 1) as i32)
            .map_err(|e| format!("Embeddings extraction failed: {:?}", e))?,
    };

    // Perform L2 Normalization
    let mut emb = raw_emb.to_vec();
    let norm = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in emb.iter_mut() {
            *v /= norm;
        }
    }

    Ok(emb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_embed_tokens_fit_cho_phep_dung_toi_n_ctx() {
        // Vừa hoặc nhỏ hơn n_ctx thì hợp lệ:
        assert!(check_embed_tokens_fit(0, 2048).is_ok());
        assert!(check_embed_tokens_fit(512, 2048).is_ok());
        assert!(check_embed_tokens_fit(2048, 2048).is_ok());

        // Vượt quá n_ctx dù chỉ 1 token cũng phải bị từ chối với thông báo rõ ràng:
        let err = check_embed_tokens_fit(2049, 2048).unwrap_err();
        assert!(err.contains("2049 token"));
        assert!(err.contains("n_ctx = 2048"));

        let err_big = check_embed_tokens_fit(50000, 4096).unwrap_err();
        assert!(err_big.contains("50000 token"));
        assert!(err_big.contains("n_ctx = 4096"));
    }
}
