//! Empirical Challenger M1.2 Test Harness: AdaptiveTurnDecision & Core Types Edge Cases
//!
//! Objectives:
//! Verify probability edge cases in `AdaptiveTurnDecision::from_probability`:
//! 1. NaN floating-point inputs: behavior under IEEE 754 non-signaling NaN.
//! 2. Negative probabilities (-0.01, -1.0, -INFINITY).
//! 3. Out-of-bounds probabilities > 1.0 (1.001, 2.5, +INFINITY).
//! 4. Precision epsilon boundary conditions around 0.50 and 0.92.
//! 5. Verification of TurnVerdict properties and weather arguments schema.

use liva_core_types::{AdaptiveTurnDecision, TurnVerdict, WeatherArgs};

#[test]
fn test_adaptive_turn_decision_nan_edge_case() {
    let nan_val = f32::NAN;
    let decision = AdaptiveTurnDecision::from_probability(nan_val);

    // Because NaN > 0.92 is false and NaN >= 0.50 is false,
    // the implementation falls back to the safe default `Incomplete`.
    assert!(
        matches!(decision, AdaptiveTurnDecision::Incomplete { probability } if probability.is_nan()),
        "NaN must fall back to Incomplete to prevent premature turn cutoff"
    );
    assert!(
        !decision.is_immediate(),
        "NaN must never trigger an immediate cutoff"
    );
    assert!(
        decision.probability().is_nan(),
        "probability() getter must report NaN"
    );
}

#[test]
fn test_adaptive_turn_decision_negative_probabilities() {
    let test_cases = [-0.0001f32, -0.5, -1.0, -100.0, f32::NEG_INFINITY];

    for &neg_p in &test_cases {
        let decision = AdaptiveTurnDecision::from_probability(neg_p);
        assert!(
            matches!(decision, AdaptiveTurnDecision::Incomplete { probability } if probability == neg_p),
            "Negative probability {neg_p} must resolve to Incomplete"
        );
        assert!(
            !decision.is_immediate(),
            "Negative probability must not be immediate"
        );
        assert_eq!(decision.probability(), neg_p);
    }
}

#[test]
fn test_adaptive_turn_decision_greater_than_one_probabilities() {
    let test_cases = [1.0001f32, 1.5, 2.0, 100.0, f32::INFINITY];

    for &large_p in &test_cases {
        let decision = AdaptiveTurnDecision::from_probability(large_p);
        assert!(
            matches!(decision, AdaptiveTurnDecision::ImmediateCutoff { probability } if probability == large_p),
            "Probability > 1.0 ({large_p}) must resolve to ImmediateCutoff"
        );
        assert!(
            decision.is_immediate(),
            "Probability > 1.0 must be immediate"
        );
        assert_eq!(decision.probability(), large_p);
    }
}

#[test]
fn test_adaptive_turn_decision_fine_grained_boundaries() {
    // Epsilon below 0.50 -> Incomplete
    let p_below_50 = 0.49999994f32;
    let d1 = AdaptiveTurnDecision::from_probability(p_below_50);
    assert!(matches!(d1, AdaptiveTurnDecision::Incomplete { .. }));
    assert!(!d1.is_immediate());

    // Exactly 0.50 -> HesitationWait
    let d2 = AdaptiveTurnDecision::from_probability(0.50);
    assert!(matches!(d2, AdaptiveTurnDecision::HesitationWait { .. }));
    assert!(!d2.is_immediate());

    // Exactly 0.92 -> HesitationWait
    let d3 = AdaptiveTurnDecision::from_probability(0.92);
    assert!(matches!(d3, AdaptiveTurnDecision::HesitationWait { .. }));
    assert!(!d3.is_immediate());

    // Epsilon above 0.92 -> ImmediateCutoff
    let p_above_92 = 0.9200001f32;
    let d4 = AdaptiveTurnDecision::from_probability(p_above_92);
    assert!(matches!(d4, AdaptiveTurnDecision::ImmediateCutoff { .. }));
    assert!(d4.is_immediate());
}

#[test]
fn test_turn_verdict_edge_values() {
    let v_zero = TurnVerdict {
        probability: 0.0,
        complete: false,
    };
    assert_eq!(v_zero.probability, 0.0);
    assert!(!v_zero.complete);

    let v_one = TurnVerdict {
        probability: 1.0,
        complete: true,
    };
    assert_eq!(v_one.probability, 1.0);
    assert!(v_one.complete);

    let v_neg = TurnVerdict {
        probability: -0.5,
        complete: false,
    };
    assert_eq!(v_neg.probability, -0.5);

    let v_large = TurnVerdict {
        probability: 2.5,
        complete: true,
    };
    assert_eq!(v_large.probability, 2.5);

    // Test copy and equality
    let copy = v_one;
    assert_eq!(v_one, copy);
}

#[test]
fn test_weather_args_empty_and_vietnamese_unicode() {
    let empty_args = WeatherArgs { location: None };
    assert_eq!(empty_args.location, None);

    let vn_args = WeatherArgs {
        location: Some("TP. Hồ Chí Minh".to_string()),
    };
    assert_eq!(vn_args.location.as_deref(), Some("TP. Hồ Chí Minh"));

    let schema = schemars::schema_for!(WeatherArgs);
    assert!(schema.schema.metadata.is_some());
}
