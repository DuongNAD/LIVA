use llama_cpp_2::sampling::LlamaSampler;

pub fn create_sampler(temperature: f32, top_p: f32) -> LlamaSampler {
    // Standard sampler parameters
    let top_k = 40;
    let min_p = 0.05;
    let seed = rand::random::<u32>();

    LlamaSampler::chain_simple([
        LlamaSampler::top_k(top_k),
        LlamaSampler::top_p(top_p, 1),
        LlamaSampler::min_p(min_p, 1),
        LlamaSampler::temp(temperature),
        LlamaSampler::dist(seed),
    ])
}

#[allow(dead_code)]
pub fn create_greedy_sampler() -> LlamaSampler {
    LlamaSampler::greedy()
}
