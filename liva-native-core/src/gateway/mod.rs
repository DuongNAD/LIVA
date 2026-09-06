//! Gateway Subsystem
//!
//! Exposes WebSocket Gateway Control Plane framing, role-based authorization,
//! node pairing, and event broadcasting.

pub mod control_plane;
pub mod pairing;

pub use control_plane::{
    ActiveNodeConnection, ControlErrorPayload, ControlFrame, ControlOpcode, GatewayControlPlane,
    GatewayError, InMemoryGatewayControlPlane,
};
pub use pairing::{
    ApprovedNodeRecord, NodeId, NodeRole, PairingChallenge, PairingRegistry, PairingRequest,
    PairingResponse,
};
