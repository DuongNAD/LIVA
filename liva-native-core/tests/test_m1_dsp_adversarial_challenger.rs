//! Adversarial Empirical Challenger Test Suite for Milestone 1: Realtime Audio DSP Subsystem.
//!
//! Subsystems under test:
//! 1. HighPassFilter (80Hz Butterworth Biquad) & Agc (Digital AGC & Soft-Knee Limiter)
//! 2. BandlimitedResampler (Sinc polyphase resampler 22050/24000/48000 -> 16000) & SelfEchoCanceller (AEC3)
//! 3. GtcrnDenoiser (Zero-allocation ONNX neural denoiser)
//! 4. VoiceSessionAudio (Full pipeline multi-session integration)

use liva_native_core::webrtc::aec::{BandlimitedResampler, SelfEchoCanceller};
use liva_native_core::webrtc::agc::Agc;
use liva_native_core::webrtc::denoise::{resolve_model_path as resolve_gtcrn_path, GtcrnDenoiser, HOP, WIN};
use liva_native_core::webrtc::session::VoiceSessionAudio;
use std::f32::consts::PI;

fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

fn calculate_peak(samples: &[f32]) -> f32 {
    samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max)
}

fn generate_sine(freq_hz: f32, sample_rate: u32, duration_sec: f32, amplitude: f32) -> Vec<f32> {
    let total_samples = (sample_rate as f32 * duration_sec).round() as usize;
    (0..total_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            amplitude * (2.0 * PI * freq_hz * t).sin()
        })
        .collect()
}

// ============================================================================
// SUITE 1: EXTREME AMPLITUDES, TRANSIENTS, AND NUMERICAL ANOMALIES
// ============================================================================

#[test]
fn test_adv_agc_extreme_positive_amplitude_24dbfs() {
    let mut agc = Agc::default_16k();
    // +24 dBFS sine wave (amplitude ~15.85)
    let amp_24db = 10.0f32.powf(24.0 / 20.0);
    let mut samples = generate_sine(400.0, 16000, 0.5, amp_24db);

    agc.process(&mut samples);

    let peak = calculate_peak(&samples);
    let limiter_ceiling = 10.0f32.powf(-3.0 / 20.0); // ~0.70795 (-3.0 dBFS)

    assert!(
        peak <= limiter_ceiling + 1e-4,
        "AGC limiter failed under +24 dBFS input: peak was {} (limit {})",
        peak,
        limiter_ceiling
    );
    assert!(samples.iter().all(|s| s.is_finite()));
}

#[test]
fn test_adv_agc_extreme_massive_overdrive_40dbfs() {
    let mut agc = Agc::default_16k();
    // Massive +40 dBFS overdrive (amplitude = 100.0)
    let mut samples = generate_sine(300.0, 16000, 0.5, 100.0);

    agc.process(&mut samples);

    let peak = calculate_peak(&samples);
    let limiter_ceiling = 10.0f32.powf(-3.0 / 20.0);

    assert!(
        peak <= limiter_ceiling + 1e-4,
        "AGC limiter must strictly contain +40 dBFS overdrive to <= -3.0 dBFS: got peak {}",
        peak
    );
    assert!(samples.iter().all(|s| s.is_finite()));
}

#[test]
fn test_adv_agc_extreme_low_amplitude_96dbfs() {
    let mut agc = Agc::default_16k();
    // -96 dBFS input (amplitude ~0.0000224)
    let amp_96db = 10.0f32.powf(-96.0 / 20.0);
    let mut samples = generate_sine(400.0, 16000, 0.5, amp_96db);

    let initial_rms = calculate_rms(&samples);
    agc.process(&mut samples);

    // Should NOT experience noise explosion: noise gate floor (-50 dBFS) must freeze gain
    let final_gain_db = agc.current_gain_db();
    assert!(
        final_gain_db < 3.0,
        "Noise gate must prevent gain expansion on -96 dBFS signal: gain was {} dB",
        final_gain_db
    );
    let final_rms = calculate_rms(&samples);
    assert!(
        final_rms < initial_rms * 2.0,
        "Output RMS must not blow up on -96 dBFS signal"
    );
}

#[test]
fn test_adv_agc_severe_dc_bias_rejection() {
    let mut agc = Agc::default_16k();
    // Pure DC step of +1.0 for 0.5s
    let mut dc_samples = vec![1.0f32; 8000];
    agc.process(&mut dc_samples);

    // After 80Hz filter convergence (e.g. second half)
    let converged_dc = &dc_samples[4000..];
    let mean_dc: f32 = converged_dc.iter().sum::<f32>() / converged_dc.len() as f32;
    assert!(
        mean_dc.abs() < 0.005,
        "80Hz HPF in AGC must eliminate DC bias (+1.0), remaining mean was {}",
        mean_dc
    );
}

#[test]
fn test_adv_agc_dirac_impulse_and_transient_bursts() {
    let mut agc = Agc::default_16k();
    let limiter_ceiling = 10.0f32.powf(-3.0 / 20.0);

    // Sequence of 10 violent Dirac impulses of amplitude 50.0 interspersed with silence
    let mut impulse_train = vec![0.0f32; 16000];
    for i in 0..10 {
        impulse_train[i * 1500] = 50.0;
        impulse_train[i * 1500 + 1] = -50.0;
    }

    agc.process(&mut impulse_train);

    let peak = calculate_peak(&impulse_train);
    assert!(
        peak <= limiter_ceiling + 1e-4,
        "Dirac impulse train must be strictly limited to <= -3.0 dBFS: got peak {}",
        peak
    );
    assert!(impulse_train.iter().all(|s| s.is_finite()));
}

#[test]
fn test_adv_agc_subnormal_and_denormal_floats() {
    let mut agc = Agc::default_16k();
    let mut subnormals = vec![1e-38f32; 1600];
    agc.process(&mut subnormals);

    assert!(subnormals.iter().all(|s| s.is_finite()));
    let peak = calculate_peak(&subnormals);
    assert!(peak < 1e-6);
}

#[test]
fn test_adv_agc_nan_and_inf_handling_and_reset_recovery() {
    let mut agc = Agc::default_16k();

    // Array containing NaN and Infs
    let mut nan_samples = vec![0.1f32; 100];
    nan_samples[10] = f32::NAN;
    nan_samples[50] = f32::INFINITY;
    nan_samples[70] = f32::NEG_INFINITY;

    // Must not panic on NaN/Inf
    agc.process(&mut nan_samples);

    // After reset, state must be cleanly cleared and operational
    agc.reset();
    let mut normal_samples = vec![0.1f32; 1600];
    agc.process(&mut normal_samples);
    assert!(normal_samples.iter().all(|s| s.is_finite()));
    assert!(agc.current_gain_db().is_finite());
}

// ============================================================================
// SUITE 2: BANDLIMITED RESAMPLER ACCURACY & ANTI-ALIASING HARNESS
// ============================================================================

#[test]
fn test_adv_resampler_sinusoidal_accuracy_across_rates() {
    // Test precise downsampling with exact frequency verification
    for &src_rate in &[22050u32, 24000u32, 48000u32] {
        for &freq in &[100.0f32, 400.0f32, 1000.0f32, 2500.0f32, 5000.0f32] {
            let in_sig = generate_sine(freq, src_rate, 0.2, 0.6);
            let out_sig = BandlimitedResampler::resample_to_16k(&in_sig, src_rate);

            let expected_len = ((in_sig.len() as f64) * (16000.0 / src_rate as f64)).round() as usize;
            assert_eq!(out_sig.len(), expected_len);

            let middle_in = &in_sig[400..in_sig.len() - 400];
            let middle_out = &out_sig[400..out_sig.len() - 400];
            let rms_in = calculate_rms(middle_in);
            let rms_out = calculate_rms(middle_out);

            let diff_db = (20.0 * (rms_out / rms_in).log10()).abs();
            assert!(
                diff_db < 1.5,
                "Frequency {}Hz from {}Hz failed passband accuracy: diff was {} dB",
                freq,
                src_rate,
                diff_db
            );
        }
    }
}

#[test]
fn test_adv_resampler_anti_aliasing_stopband_rejection_24k() {
    // 10kHz tone at 24kHz (Nyquist is 12kHz).
    // When downsampled to 16kHz (Nyquist is 8kHz), 10kHz is above 8kHz and MUST be attenuated.
    let input = generate_sine(10000.0, 24000, 0.3, 0.8);
    let output = BandlimitedResampler::resample_to_16k(&input, 24000);

    let middle = &output[400..output.len() - 400];
    let out_rms = calculate_rms(middle);
    let in_rms = calculate_rms(&input);

    let attenuation_db = 20.0 * (in_rms / (out_rms + 1e-9)).log10();
    assert!(
        attenuation_db > 25.0,
        "10kHz stopband signal must be attenuated by > 25 dB to prevent aliasing (got {} dB attenuation)",
        attenuation_db
    );
}

#[test]
fn test_adv_resampler_anti_aliasing_stopband_rejection_48k() {
    // 18kHz tone at 48kHz.
    // When downsampled to 16kHz, 18kHz MUST be heavily attenuated.
    let input = generate_sine(18000.0, 48000, 0.3, 0.8);
    let output = BandlimitedResampler::resample_to_16k(&input, 48000);

    let middle = &output[400..output.len() - 400];
    let out_rms = calculate_rms(middle);
    let in_rms = calculate_rms(&input);

    let attenuation_db = 20.0 * (in_rms / (out_rms + 1e-9)).log10();
    assert!(
        attenuation_db > 40.0,
        "18kHz stopband signal from 48kHz must be attenuated by > 40 dB (got {} dB attenuation)",
        attenuation_db
    );
}

#[test]
fn test_adv_resampler_boundary_empty_and_identity() {
    let empty: Vec<f32> = Vec::new();
    let res_empty = BandlimitedResampler::resample_to_16k(&empty, 48000);
    assert!(res_empty.is_empty());

    let native = vec![0.1f32; 1600];
    let res_native = BandlimitedResampler::resample_to_16k(&native, 16000);
    assert_eq!(res_native, native);
}

// ============================================================================
// SUITE 3: GTCRN DENOISER RAPID STREAMING BURSTS & STRESS
// ============================================================================

#[test]
fn test_adv_gtcrn_rapid_variable_chunk_streaming() {
    let model_path = resolve_gtcrn_path();
    if !model_path.exists() {
        eprintln!("skip: gtcrn_simple.onnx not present");
        return;
    }

    let mut denoiser = GtcrnDenoiser::new(&model_path).expect("load GTCRN");
    let test_signal = generate_sine(400.0, 16000, 0.5, 0.3);

    // Stream with irregular, adversarial chunk sizes: 1, 7, 13, 100, 255, 256, 300, 512, 1000
    let chunk_sizes = [1, 7, 13, 100, 255, 256, 300, 512, 1000];
    let mut cursor = 0;
    let mut step = 0;
    let mut total_emitted = 0;

    while cursor < test_signal.len() {
        let size = chunk_sizes[step % chunk_sizes.len()];
        let end = (cursor + size).min(test_signal.len());
        let chunk = &test_signal[cursor..end];
        cursor = end;
        step += 1;

        let out = denoiser.process_audio(chunk).expect("process_audio irregular chunk");
        assert!(out.iter().all(|s| s.is_finite()));
        total_emitted += out.len();
    }

    assert!(total_emitted + WIN >= test_signal.len());
}

#[test]
fn test_adv_gtcrn_rapid_turn_resets_under_load() {
    let model_path = resolve_gtcrn_path();
    if !model_path.exists() {
        eprintln!("skip: gtcrn_simple.onnx not present");
        return;
    }

    let mut denoiser = GtcrnDenoiser::new(&model_path).expect("load GTCRN");
    let chunk = vec![0.2f32; HOP];

    // Rapidly alternate between processing and reset (simulating 50 rapid barge-in turn cuts)
    for _ in 0..50 {
        let out = denoiser.process_audio(&chunk).expect("process hop");
        assert!(out.iter().all(|s| s.is_finite()));
        denoiser.reset();
    }
}

#[test]
fn test_adv_gtcrn_multi_session_concurrent_stress() {
    let model_path = resolve_gtcrn_path();
    if !model_path.exists() {
        eprintln!("skip: gtcrn_simple.onnx not present");
        return;
    }

    let prototype = GtcrnDenoiser::new(&model_path).expect("load GTCRN prototype");

    // Create 8 virtual concurrent WebSocket audio streams sharing the ONNX session
    let mut sessions: Vec<GtcrnDenoiser> = (0..8).map(|_| prototype.fork_session()).collect();

    for iteration in 0..20 {
        for (i, session) in sessions.iter_mut().enumerate() {
            let freq = 200.0 + (i as f32 * 100.0);
            let frame = generate_sine(freq, 16000, 0.016, 0.4); // 16ms = 256 samples
            let out = session.process_audio(&frame).expect("concurrent session process");
            assert!(out.iter().all(|s| s.is_finite()));
            if iteration % 5 == 0 {
                session.reset();
            }
        }
    }
}

// ============================================================================
// SUITE 4: FULL DSP PIPELINE MULTI-SESSION ISOLATION
// ============================================================================

#[test]
fn test_adv_full_pipeline_multi_session_stress() {
    let session1 = VoiceSessionAudio::new(
        None,
        None,
        Some(SelfEchoCanceller::new()),
        Some(Agc::default_16k()),
    );
    let session2 = VoiceSessionAudio::new(
        None,
        None,
        Some(SelfEchoCanceller::new()),
        Some(Agc::default_16k()),
    );

    // Session 1: Extreme loud audio (+20 dBFS)
    let loud = vec![10.0f32; 1600];
    let (_, out1) = session1.process_mic(loud).expect("process mic loud");
    let peak1 = calculate_peak(&out1);
    assert!(peak1 <= 10.0f32.powf(-3.0 / 20.0) + 1e-4);

    // Session 2: Quiet audio (-40 dBFS)
    let quiet = vec![0.01f32; 1600];
    let (_, out2) = session2.process_mic(quiet).expect("process mic quiet");
    assert!(out2.iter().all(|s| s.is_finite()));
}
