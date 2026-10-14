use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WeatherArgs {
    /// Tên địa điểm nếu người dùng nói rõ. Bỏ trống để dùng profile hoặc vị trí IP đã opt-in.
    #[serde(default)]
    pub location: Option<String>,
}

/// Turn verdict with probability and completion flag.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TurnVerdict {
    pub probability: f32,
    pub complete: bool,
}

/// Adaptive end-of-turn decision for the Two-Stage Turn Gate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AdaptiveTurnDecision {
    /// Turn clearly complete (p > 0.92). Immediate cutoff at ~200ms silence.
    ImmediateCutoff { probability: f32 },
    /// Hesitation / thinking pause (0.50 <= p <= 0.92). Extend silence window up to ~450ms.
    HesitationWait { probability: f32 },
    /// User still actively speaking / incomplete (p < 0.50). Continue accumulating.
    Incomplete { probability: f32 },
}

impl AdaptiveTurnDecision {
    pub fn from_probability(p: f32) -> Self {
        if p > 0.92 {
            Self::ImmediateCutoff { probability: p }
        } else if p >= 0.50 {
            Self::HesitationWait { probability: p }
        } else {
            Self::Incomplete { probability: p }
        }
    }

    pub fn probability(&self) -> f32 {
        match *self {
            Self::ImmediateCutoff { probability }
            | Self::HesitationWait { probability }
            | Self::Incomplete { probability } => probability,
        }
    }

    pub fn is_immediate(&self) -> bool {
        matches!(self, Self::ImmediateCutoff { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adaptive_turn_decision_boundaries() {
        // Boundary: 0.0 -> Incomplete
        let d_zero = AdaptiveTurnDecision::from_probability(0.0);
        assert!(
            matches!(d_zero, AdaptiveTurnDecision::Incomplete { probability } if (probability - 0.0).abs() < f32::EPSILON)
        );
        assert!(!d_zero.is_immediate());
        assert_eq!(d_zero.probability(), 0.0);

        // Boundary: 0.49 -> Incomplete
        let d_49 = AdaptiveTurnDecision::from_probability(0.49);
        assert!(
            matches!(d_49, AdaptiveTurnDecision::Incomplete { probability } if (probability - 0.49).abs() < f32::EPSILON)
        );
        assert!(!d_49.is_immediate());
        assert_eq!(d_49.probability(), 0.49);

        // Boundary: 0.50 -> HesitationWait
        let d_50 = AdaptiveTurnDecision::from_probability(0.50);
        assert!(
            matches!(d_50, AdaptiveTurnDecision::HesitationWait { probability } if (probability - 0.50).abs() < f32::EPSILON)
        );
        assert!(!d_50.is_immediate());
        assert_eq!(d_50.probability(), 0.50);

        // Boundary: 0.92 -> HesitationWait
        let d_92 = AdaptiveTurnDecision::from_probability(0.92);
        assert!(
            matches!(d_92, AdaptiveTurnDecision::HesitationWait { probability } if (probability - 0.92).abs() < f32::EPSILON)
        );
        assert!(!d_92.is_immediate());
        assert_eq!(d_92.probability(), 0.92);

        // Boundary: 0.93 -> ImmediateCutoff
        let d_93 = AdaptiveTurnDecision::from_probability(0.93);
        assert!(
            matches!(d_93, AdaptiveTurnDecision::ImmediateCutoff { probability } if (probability - 0.93).abs() < f32::EPSILON)
        );
        assert!(d_93.is_immediate());
        assert_eq!(d_93.probability(), 0.93);

        // Boundary: 1.0 -> ImmediateCutoff
        let d_one = AdaptiveTurnDecision::from_probability(1.0);
        assert!(
            matches!(d_one, AdaptiveTurnDecision::ImmediateCutoff { probability } if (probability - 1.0).abs() < f32::EPSILON)
        );
        assert!(d_one.is_immediate());
        assert_eq!(d_one.probability(), 1.0);
    }

    #[test]
    fn test_turn_verdict_properties_and_serde() {
        let verdict = TurnVerdict {
            probability: 0.88,
            complete: true,
        };
        assert_eq!(verdict.probability, 0.88);
        assert!(verdict.complete);

        let verdict_copy = verdict;
        assert_eq!(verdict, verdict_copy);
        assert_eq!(
            format!("{verdict:?}"),
            "TurnVerdict { probability: 0.88, complete: true }"
        );
    }

    #[test]
    fn test_weather_args_schema_generation() {
        let schema = schemars::schema_for!(WeatherArgs);
        let metadata = schema.schema.metadata.as_ref().expect("schema metadata");
        assert_eq!(metadata.title.as_deref(), Some("WeatherArgs"));

        let object_validation = schema.schema.object.as_ref().expect("object validation");
        assert!(object_validation.properties.contains_key("location"));

        let args = WeatherArgs {
            location: Some("Hà Nội".to_string()),
        };
        assert_eq!(args.location.as_deref(), Some("Hà Nội"));
    }
}
