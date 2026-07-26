use rustfft::{FftPlanner, num_complex::Complex};

fn hann_window(size: usize) -> Vec<f32> {
    (0..size)
        .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (size as f32 - 1.0)).cos()))
        .collect()
}

pub fn extract_style_vector(audio_data: &[f32]) -> Vec<f32> {
    let window_size = 512;
    let hop_length = 256;

    let mut padded_audio = audio_data.to_vec();
    if padded_audio.len() < window_size {
        padded_audio.resize(window_size, 0.0);
    }

    let num_frames = 1 + (padded_audio.len().saturating_sub(window_size)) / hop_length;
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(window_size);
    let window = hann_window(window_size);

    let mut magnitudes_sum = vec![0.0f32; 256];
    let mut frame_count = 0;

    for i in 0..num_frames {
        let start = i * hop_length;
        let mut buffer: Vec<Complex<f32>> = padded_audio[start..start + window_size]
            .iter()
            .zip(&window)
            .map(|(&x, &w)| Complex { re: x * w, im: 0.0 })
            .collect();

        fft.process(&mut buffer);

        for j in 0..256 {
            magnitudes_sum[j] += buffer[j].norm();
        }
        frame_count += 1;
    }

    let mut style_vector: Vec<f32> = magnitudes_sum
        .iter()
        .map(|&sum| sum / frame_count as f32)
        .collect();

    let is_silent =
        style_vector.iter().all(|&x| x == 0.0) || style_vector.iter().any(|x| x.is_nan());
    if is_silent {
        style_vector = vec![0.1f32; 256];
    }

    let mean: f32 = style_vector.iter().sum::<f32>() / 256.0;
    let variance: f32 = style_vector
        .iter()
        .map(|&x| (x - mean).powi(2))
        .sum::<f32>()
        / 256.0;
    let std_dev = variance.sqrt();

    for x in style_vector.iter_mut() {
        if std_dev > 1e-6 {
            *x = (*x - mean) / std_dev;
        } else {
            *x -= mean;
        }
        *x = *x * 0.185 - 0.005;
    }

    let mut final_profile = Vec::with_capacity(511 * 256);
    for _ in 0..511 {
        final_profile.extend_from_slice(&style_vector);
    }

    final_profile
}
