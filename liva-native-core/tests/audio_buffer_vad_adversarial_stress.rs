use liva_native_core::webrtc::session::{TurnAudioAction, TurnAudioBuffer};
use liva_native_core::webrtc::vad::{VadConfig, VadEngine, VadEvent, resolve_model_path};
use std::time::Instant;

fn get_vad_model_path() -> std::path::PathBuf {
    let mut model_dir =
        std::env::var("LIVA_STT_MODEL_DIR").unwrap_or_else(|_| "models/nemotron-asr".to_string());
    if !std::path::Path::new(&model_dir).exists() {
        model_dir = "../models/nemotron-asr".to_string();
    }
    resolve_model_path(&model_dir)
}

// ─────────────────────────────────────────────────────────────────────────────
// Test Suite 1: Continuous Noise 60s (960,000 samples)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn challenge_continuous_noise_60s_sample_continuity_and_termination() {
    // 60 seconds of audio @ 16 kHz = 960,000 samples.
    const SAMPLE_RATE: usize = 16_000;
    const TOTAL_SECONDS: usize = 60;
    const TOTAL_SAMPLES: usize = SAMPLE_RATE * TOTAL_SECONDS; // 960,000 samples
    const CHUNK_SIZE: usize = 1600; // 100ms chunks
    const TOTAL_CHUNKS: usize = TOTAL_SAMPLES / CHUNK_SIZE; // 600 chunks

    let mut buffer = TurnAudioBuffer::new(1536);

    // Generate unique sequential audio samples to verify zero sample drops
    let mut all_samples = Vec::with_capacity(TOTAL_SAMPLES);
    for i in 0..TOTAL_SAMPLES {
        all_samples.push((i as f32) + 0.1);
    }

    let mut started_count = 0usize;
    let mut ended_turns: Vec<Vec<f32>> = Vec::new();
    let mut actions_per_chunk: Vec<usize> = Vec::with_capacity(TOTAL_CHUNKS);

    let start_time = Instant::now();

    // VAD enters speech mode on chunk 0
    for chunk_idx in 0..TOTAL_CHUNKS {
        let chunk_slice = &all_samples[chunk_idx * CHUNK_SIZE..(chunk_idx + 1) * CHUNK_SIZE];
        let events = if chunk_idx == 0 {
            vec![VadEvent::SpeechStart]
        } else {
            // VAD is stuck in speech mode: no new SpeechStart, no SpeechEnd
            vec![]
        };

        let actions = buffer.ingest(chunk_slice, &events);
        actions_per_chunk.push(actions.len());

        for action in actions {
            match action {
                TurnAudioAction::Started => started_count += 1,
                TurnAudioAction::Ended(audio) => ended_turns.push(audio),
                TurnAudioAction::SilenceProbe { .. } => {}
            }
        }
    }

    let elapsed = start_time.elapsed();
    println!(
        "[Continuous Noise 60s] Ingestion elapsed: {:?}, Started actions: {}, Ended turns: {}",
        elapsed,
        started_count,
        ended_turns.len()
    );

    // 1. Verify that continuous noise across 60 seconds segments into exactly 3 turns (every 20s)
    assert_eq!(
        ended_turns.len(),
        3,
        "Continuous noise across 60s must segment into exactly 3 turns (20s, 40s, 60s)"
    );
    // Silent re-arming must NOT emit Started actions on ceiling transitions (Started count remains 1)
    assert_eq!(
        started_count, 1,
        "Must emit exactly 1 initial Started action (silent re-arming emits no Started)"
    );

    // 2. Verify bit-for-bit sample continuity across ALL 3 turns (960,000 samples)
    let mut total_verified_samples = 0;
    for (turn_idx, turn) in ended_turns.iter().enumerate() {
        assert_eq!(
            turn.len(),
            TurnAudioBuffer::MAX_TURN_SAMPLES,
            "Turn {} must have exactly MAX_TURN_SAMPLES (320,000 samples)",
            turn_idx + 1
        );
        for (i, &sample) in turn.iter().enumerate() {
            let global_idx = total_verified_samples + i;
            let expected = (global_idx as f32) + 0.1;
            assert_eq!(
                sample,
                expected,
                "Sample mismatch in turn {} at sample {}: expected {}, got {}",
                turn_idx + 1,
                i,
                expected,
                sample
            );
        }
        total_verified_samples += turn.len();
    }
    assert_eq!(
        total_verified_samples, TOTAL_SAMPLES,
        "All 960,000 samples must be bit-for-bit preserved across the 3 turns (zero dropped samples)"
    );

    println!(
        "[Continuous Noise 60s] Sample continuity verified: 0 samples dropped across all 3 turns (960,000 samples)"
    );

    // 3. Check memory boundedness:
    // Buffer is armed for Turn 4; extra chunk accumulates without premature action or panic
    let extra_chunk = vec![0.5f32; 1600];
    let extra_actions = buffer.ingest(&extra_chunk, &[]);
    assert!(
        extra_actions.is_empty(),
        "Extra chunk in Turn 4 must emit 0 actions before reaching ceiling"
    );
}

#[test]
fn challenge_stuck_vad_subsequent_speech_is_preserved_after_ceiling() {
    // Verifies that when VAD is stuck in speech mode, TurnAudioBuffer terminates
    // Turn 1 at 20s, silently re-arms, and preserves subsequent user speech
    let mut buffer = TurnAudioBuffer::new(1536);
    let chunk = vec![0.1f32; 1600];

    // Initial speech start
    let start_actions = buffer.ingest(&chunk, &[VadEvent::SpeechStart]);
    assert_eq!(start_actions, vec![TurnAudioAction::Started]);

    // Feed 199 chunks to reach 200 * 1600 = 320,000 samples (20.0s)
    let mut turn1_ended = None;
    for _ in 1..200 {
        let acts = buffer.ingest(&chunk, &[]);
        for a in acts {
            if let TurnAudioAction::Ended(audio) = a {
                turn1_ended = Some(audio);
            }
        }
    }
    assert!(turn1_ended.is_some(), "Turn 1 must end at 20s");

    // At t = 20s, user begins speaking for 5 seconds (50 chunks * 1600 = 80,000 samples)
    // while ambient noise keeps VAD in speech mode (no SpeechStart, no SpeechEnd)
    let speech_chunk = vec![0.9f32; 1600]; // User command voice
    for _ in 0..50 {
        let acts = buffer.ingest(&speech_chunk, &[]);
        // During continuous speech before ceiling, no intermediate actions
        assert!(acts.is_empty());
    }

    // User finishes speaking; VAD detects silence and emits SpeechEnd
    let end_actions = buffer.ingest(&speech_chunk, &[VadEvent::SpeechEnd]);
    assert_eq!(end_actions.len(), 1);
    let [TurnAudioAction::Ended(user_audio)] = end_actions.as_slice() else {
        panic!("SpeechEnd must cleanly emit completed user utterance");
    };

    // 50 chunks * 1600 samples + 1 end chunk * 1600 samples = 51 * 1600 = 81,600 samples
    assert_eq!(user_audio.len(), 51 * 1600);
    assert!(
        user_audio.iter().all(|&s| (s - 0.9f32).abs() < 1e-6),
        "All user voice samples must be preserved with zero corruption or loss"
    );
}

#[test]
fn challenge_continuous_noise_with_vad_state_machine_60s() {
    let model_path = get_vad_model_path();
    if !model_path.exists() {
        eprintln!("skip: Silero VAD model not found at {:?}", model_path);
        return;
    }

    let config = VadConfig {
        frame_size: 256,
        speech_start_threshold: 3,
        speech_end_threshold: 22,
        ..VadConfig::default()
    };
    let mut vad_engine = VadEngine::new(&model_path, config).expect("init VadEngine");
    let mut turn_audio = TurnAudioBuffer::new(1536);

    // 60 seconds @ 16kHz = 960,000 samples / 256 samples per frame = 3750 frames
    const TOTAL_FRAMES: usize = (16_000 * 60) / 256;
    let frame = vec![0.1f32; 256];

    let mut all_vad_events = Vec::new();
    let mut ended_turns = Vec::new();
    let mut started_count = 0usize;

    for frame_idx in 0..TOTAL_FRAMES {
        // Continuous speech/noise signal: confidence remains high, VAD is stuck in speech mode
        let event = vad_engine.test_update_state_machine(true);
        let events = match event {
            Some(e) => {
                all_vad_events.push((frame_idx, e));
                vec![e]
            }
            None => vec![],
        };

        let actions = turn_audio.ingest(&frame, &events);
        for action in actions {
            match action {
                TurnAudioAction::Started => started_count += 1,
                TurnAudioAction::Ended(audio) => ended_turns.push(audio),
                TurnAudioAction::SilenceProbe { .. } => {}
            }
        }
    }

    println!(
        "[VAD State Machine 60s Noise] VAD events total: {}, Started: {}, Ended turns: {}",
        all_vad_events.len(),
        started_count,
        ended_turns.len()
    );

    // Confirm VAD fired SpeechStart initially at frame 2 (after 3 consecutive speech frames)
    assert_eq!(
        all_vad_events.len(),
        1,
        "VAD must emit SpeechStart exactly once when stuck in speech mode"
    );
    assert_eq!(all_vad_events[0].1, VadEvent::SpeechStart);
    assert!(
        vad_engine.is_speaking(),
        "VAD must remain in speech mode throughout"
    );

    // Check turns ended: 60s noise through live VAD state machine must segment into exactly 3 turns
    assert_eq!(
        ended_turns.len(),
        3,
        "TurnAudioBuffer with live VAD must segment 60s noise into exactly 3 turns"
    );
    assert_eq!(
        started_count, 1,
        "Must emit 1 initial Started action (silent re-arming emits no Started)"
    );
    for (idx, turn) in ended_turns.iter().enumerate() {
        assert_eq!(
            turn.len(),
            TurnAudioBuffer::MAX_TURN_SAMPLES,
            "Turn {} must terminate at exactly 320,000 samples (20s)",
            idx + 1
        );
    }

    println!(
        "[VAD State Machine 60s Noise] Total turns emitted across 60 seconds: {}",
        ended_turns.len()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test Suite 2: Rapid Oscillation of VadEvent (500+ cycles)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn challenge_rapid_vad_event_oscillation_500_cycles_across_chunks() {
    let mut buffer = TurnAudioBuffer::new(1536);
    const CYCLES: usize = 500;
    const CHUNK_LEN: usize = 800; // 800 start + 800 end = 1600 samples (>= MIN_TURN_SAMPLES)

    let mut total_started = 0usize;
    let mut total_ended = 0usize;
    let mut total_samples_emitted = 0usize;

    for cycle in 0..CYCLES {
        let chunk_start = vec![(cycle as f32) * 2.0; CHUNK_LEN];
        let chunk_end = vec![(cycle as f32) * 2.0 + 1.0; CHUNK_LEN];

        // Step 1: SpeechStart chunk
        let start_actions = buffer.ingest(&chunk_start, &[VadEvent::SpeechStart]);
        for act in start_actions {
            if act == TurnAudioAction::Started {
                total_started += 1;
            }
        }

        // Step 2: SpeechEnd chunk
        let end_actions = buffer.ingest(&chunk_end, &[VadEvent::SpeechEnd]);
        for act in end_actions {
            if let TurnAudioAction::Ended(audio) = act {
                total_ended += 1;
                total_samples_emitted += audio.len();
                // Audio must contain both chunk_start and chunk_end
                assert!(
                    audio.len() >= CHUNK_LEN * 2,
                    "Completed turn must contain at least both start and end chunks"
                );
            }
        }
    }

    println!(
        "[Rapid Oscillation 500 Cycles] Total Started: {}, Total Ended: {}, Total Samples: {}",
        total_started, total_ended, total_samples_emitted
    );

    assert_eq!(
        total_started, CYCLES,
        "Must cleanly emit exactly 500 Started actions"
    );
    assert_eq!(
        total_ended, CYCLES,
        "Must cleanly emit exactly 500 Ended actions"
    );

    // Verify buffer is completely clean after 500 cycles
    let idle_chunk = vec![0.0f32; 1600];
    let idle_actions = buffer.ingest(&idle_chunk, &[]);
    assert!(
        idle_actions.is_empty(),
        "No actions should be emitted in idle state"
    );
}

#[test]
fn challenge_intra_chunk_rapid_oscillation_500_cycles() {
    let mut buffer = TurnAudioBuffer::new(1536);
    const CYCLES: usize = 500;
    const CHUNK_LEN: usize = 1600; // >= MIN_TURN_SAMPLES

    let mut total_started = 0usize;
    let mut total_ended = 0usize;

    for cycle in 0..CYCLES {
        let chunk = vec![cycle as f32; CHUNK_LEN];
        // Both SpeechStart and SpeechEnd within the SAME chunk
        let actions = buffer.ingest(&chunk, &[VadEvent::SpeechStart, VadEvent::SpeechEnd]);

        for act in actions {
            match act {
                TurnAudioAction::Started => total_started += 1,
                TurnAudioAction::Ended(audio) => {
                    total_ended += 1;
                    assert!(
                        audio.len() >= CHUNK_LEN,
                        "Ended turn must contain at least the chunk samples"
                    );
                }
                TurnAudioAction::SilenceProbe { .. } => {}
            }
        }
    }

    println!(
        "[Intra-Chunk 500 Cycles] Total Started: {}, Total Ended: {}",
        total_started, total_ended
    );

    assert_eq!(total_started, CYCLES);
    assert_eq!(total_ended, CYCLES);
}

#[test]
fn challenge_idempotent_vad_event_floods() {
    let mut buffer = TurnAudioBuffer::new(1536);
    let chunk = vec![1.0f32; 1600]; // >= MIN_TURN_SAMPLES

    // Flood 100 consecutive SpeechStart events in a single call
    let start_flood = vec![VadEvent::SpeechStart; 100];
    let actions = buffer.ingest(&chunk, &start_flood);
    let starts = actions
        .iter()
        .filter(|&a| *a == TurnAudioAction::Started)
        .count();
    assert_eq!(
        starts, 1,
        "Must emit exactly 1 Started despite 100 SpeechStart events"
    );

    // Flood 100 consecutive SpeechStart events on next chunk while already active
    let actions2 = buffer.ingest(&chunk, &start_flood);
    assert!(
        actions2.is_empty(),
        "Must ignore redundant SpeechStart events when already active"
    );

    // Flood 100 consecutive SpeechEnd events
    let end_flood = vec![VadEvent::SpeechEnd; 100];
    let actions3 = buffer.ingest(&chunk, &end_flood);
    let ends = actions3
        .iter()
        .filter(|&a| matches!(a, TurnAudioAction::Ended(_)))
        .count();
    assert_eq!(
        ends, 1,
        "Must emit exactly 1 Ended despite 100 SpeechEnd events"
    );

    // Flood 100 consecutive SpeechEnd events when already idle
    let actions4 = buffer.ingest(&chunk, &end_flood);
    assert!(
        actions4.is_empty(),
        "Must ignore redundant SpeechEnd events when already idle"
    );
}

#[test]
fn challenge_erratic_alternating_burst_flood() {
    let mut buffer = TurnAudioBuffer::new(1536);
    let chunk = vec![0.42f32; 1600]; // >= MIN_TURN_SAMPLES

    // 50 iterations of alternating bursts
    for _ in 0..50 {
        // Burst of 10 starts
        let actions_start = buffer.ingest(&chunk, &[VadEvent::SpeechStart; 10]);
        assert_eq!(actions_start, vec![TurnAudioAction::Started]);

        // Middle idle chunk
        let actions_mid = buffer.ingest(&chunk, &[]);
        assert!(actions_mid.is_empty());

        // Burst of 10 ends
        let actions_end = buffer.ingest(&chunk, &[VadEvent::SpeechEnd; 10]);
        assert_eq!(actions_end.len(), 1);
        assert!(matches!(actions_end[0], TurnAudioAction::Ended(_)));

        // Trailing idle chunk
        let actions_idle = buffer.ingest(&chunk, &[]);
        assert!(actions_idle.is_empty());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test Suite 3: Continuous Audio 100s Extended Stress (1,600,000 samples)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn challenge_continuous_noise_100s_stress_and_bounded_memory() {
    // 100 seconds of audio @ 16 kHz = 1,600,000 samples.
    const SAMPLE_RATE: usize = 16_000;
    const TOTAL_SECONDS: usize = 100;
    const TOTAL_SAMPLES: usize = SAMPLE_RATE * TOTAL_SECONDS; // 1,600,000 samples
    const CHUNK_SIZE: usize = 1600; // 100ms chunks
    const TOTAL_CHUNKS: usize = TOTAL_SAMPLES / CHUNK_SIZE; // 1000 chunks

    let mut buffer = TurnAudioBuffer::new(1536);

    // Generate unique sequential audio samples to verify bit-for-bit continuity
    let mut all_samples = Vec::with_capacity(TOTAL_SAMPLES);
    for i in 0..TOTAL_SAMPLES {
        all_samples.push((i as f32) + 0.25);
    }

    let mut started_count = 0usize;
    let mut ended_turns: Vec<Vec<f32>> = Vec::new();

    let start_time = Instant::now();

    // VAD enters speech mode on chunk 0 and remains stuck in speech mode throughout all 100s
    for chunk_idx in 0..TOTAL_CHUNKS {
        let chunk_slice = &all_samples[chunk_idx * CHUNK_SIZE..(chunk_idx + 1) * CHUNK_SIZE];
        let events = if chunk_idx == 0 {
            vec![VadEvent::SpeechStart]
        } else {
            // VAD is stuck in speech mode: no new SpeechStart, no SpeechEnd
            vec![]
        };

        let actions = buffer.ingest(chunk_slice, &events);

        for action in actions {
            match action {
                TurnAudioAction::Started => started_count += 1,
                TurnAudioAction::Ended(audio) => ended_turns.push(audio),
                TurnAudioAction::SilenceProbe { .. } => {}
            }
        }
    }

    let elapsed = start_time.elapsed();
    println!(
        "[Continuous Noise 100s] Ingestion elapsed: {:?}, Started actions: {}, Ended turns: {}",
        elapsed,
        started_count,
        ended_turns.len()
    );

    // 1. Verify exactly 5 turns are emitted across 100 seconds (at 20s, 40s, 60s, 80s, 100s)
    assert_eq!(
        ended_turns.len(),
        5,
        "Continuous audio across 100s must segment into exactly 5 turns (every 20s)"
    );

    // 2. Verify silent re-arming: exactly 1 initial Started action is emitted across all 100 seconds
    assert_eq!(
        started_count, 1,
        "Must emit exactly 1 initial Started action (silent re-arming emits 0 Started on ceiling boundaries)"
    );

    // 3. Verify bit-for-bit sample continuity across ALL 5 turns (1,600,000 samples)
    let mut total_verified_samples = 0;
    for (turn_idx, turn) in ended_turns.iter().enumerate() {
        assert_eq!(
            turn.len(),
            TurnAudioBuffer::MAX_TURN_SAMPLES,
            "Turn {} must contain exactly MAX_TURN_SAMPLES (320,000 samples)",
            turn_idx + 1
        );
        for (i, &sample) in turn.iter().enumerate() {
            let global_idx = total_verified_samples + i;
            let expected = (global_idx as f32) + 0.25;
            assert_eq!(
                sample,
                expected,
                "Sample mismatch in turn {} at sample {}: expected {}, got {}",
                turn_idx + 1,
                i,
                expected,
                sample
            );
        }
        total_verified_samples += turn.len();
    }
    assert_eq!(
        total_verified_samples, TOTAL_SAMPLES,
        "All 1,600,000 samples must be bit-for-bit preserved across the 5 turns (zero dropped samples)"
    );

    // 4. Memory boundedness & post-100s stability check:
    // Buffer has silently re-armed for Turn 6; feed an extra chunk and verify it accumulates without premature action or panic
    let post_chunk = vec![0.75f32; 1600];
    let post_actions = buffer.ingest(&post_chunk, &[]);
    assert!(
        post_actions.is_empty(),
        "Extra chunk in Turn 6 must emit 0 actions before ceiling"
    );

    // Terminate Turn 6 with SpeechEnd and verify clean capture
    let end_chunk = vec![0.75f32; 1600];
    let end_actions = buffer.ingest(&end_chunk, &[VadEvent::SpeechEnd]);
    assert_eq!(end_actions.len(), 1);
    let [TurnAudioAction::Ended(turn6_audio)] = end_actions.as_slice() else {
        panic!("SpeechEnd must emit Turn 6");
    };
    assert_eq!(turn6_audio.len(), 3200); // 1600 from post_chunk + 1600 from end_chunk
    assert!(
        turn6_audio.iter().all(|&s| (s - 0.75f32).abs() < 1e-6),
        "Turn 6 audio samples must be intact"
    );

    println!(
        "[Continuous Noise 100s] Stress test PASSED: 5 turns emitted, 1,600,000 samples bit-verified, memory bounded, 0 panics"
    );
}

#[test]
fn challenge_continuous_noise_100s_small_frames_256_samples() {
    // 100 seconds @ 16 kHz = 1,600,000 samples with 256-sample (16ms) frames = 6250 frames
    const TOTAL_SAMPLES: usize = 16_000 * 100;
    const FRAME_SIZE: usize = 256;
    const TOTAL_FRAMES: usize = TOTAL_SAMPLES / FRAME_SIZE; // 6250 frames

    let mut buffer = TurnAudioBuffer::new(1536);
    let frame = vec![0.33f32; FRAME_SIZE];

    let mut started_count = 0usize;
    let mut ended_turns = Vec::new();

    let start_time = Instant::now();

    for frame_idx in 0..TOTAL_FRAMES {
        let events = if frame_idx == 0 {
            vec![VadEvent::SpeechStart]
        } else {
            vec![]
        };

        let actions = buffer.ingest(&frame, &events);
        for action in actions {
            match action {
                TurnAudioAction::Started => started_count += 1,
                TurnAudioAction::Ended(audio) => ended_turns.push(audio),
                TurnAudioAction::SilenceProbe { .. } => {}
            }
        }
    }

    let elapsed = start_time.elapsed();
    println!(
        "[Continuous Noise 100s / 256 frames] Ingestion elapsed: {:?}, Started: {}, Ended turns: {}",
        elapsed,
        started_count,
        ended_turns.len()
    );

    assert_eq!(
        ended_turns.len(),
        5,
        "Must segment into exactly 5 turns even with 256-sample frames"
    );
    assert_eq!(started_count, 1, "Must emit exactly 1 Started action");

    for (idx, turn) in ended_turns.iter().enumerate() {
        assert_eq!(
            turn.len(),
            TurnAudioBuffer::MAX_TURN_SAMPLES,
            "Turn {} must contain exactly 320,000 samples",
            idx + 1
        );
        assert!(
            turn.iter().all(|&s| (s - 0.33f32).abs() < 1e-6),
            "Turn {} sample values must be intact",
            idx + 1
        );
    }
}
