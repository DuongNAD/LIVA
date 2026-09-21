use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WeatherArgs {
    /// Tên địa điểm nếu người dùng nói rõ. Bỏ trống để dùng profile hoặc vị trí IP đã opt-in.
    #[serde(default)]
    pub location: Option<String>,
}

/// Turn verdict with probability and completion flag.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurnVerdict {
    pub probability: f32,
    pub complete: bool,
}

/// Adaptive end-of-turn decision for the Two-Stage Turn Gate.
#[derive(Debug, Clone, Copy, PartialEq)]
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
