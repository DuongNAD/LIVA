use liva_native_core::webrtc::session::{VoiceRuntimeComponents, VoiceRuntimeConfig};

#[test]
fn disabled_optional_processors_do_not_load_models() {
    let components = VoiceRuntimeComponents::load(
        "non-existent-stt-model",
        VoiceRuntimeConfig {
            vad_enabled: false,
            denoise_enabled: false,
            turn_shadow_enabled: false,
            aec_enabled: false,
        },
    );

    assert!(components.vad.is_none());
    assert!(components.denoiser.is_none());
    assert!(components.turn_shadow.is_none());
    assert!(components.aec.is_none());
}
