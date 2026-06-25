use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;

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
