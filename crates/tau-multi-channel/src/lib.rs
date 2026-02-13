//! Multi-channel transport runtime building blocks for Tau.
//!
//! Provides connector, routing, lifecycle, ingress, outbound, policy, and
//! telemetry components for Telegram/Discord/WhatsApp-style channels.

pub mod multi_channel_contract;
pub mod multi_channel_credentials;
pub mod multi_channel_lifecycle;
pub mod multi_channel_live_connectors;
pub mod multi_channel_live_ingress;
pub mod multi_channel_media;
pub mod multi_channel_outbound;
pub mod multi_channel_policy;
pub mod multi_channel_routing;
pub mod multi_channel_runtime;
pub mod multi_channel_send;

pub use multi_channel_contract::*;
pub use multi_channel_credentials::*;
pub use multi_channel_lifecycle::*;
pub use multi_channel_live_connectors::*;
pub use multi_channel_live_ingress::*;
pub use multi_channel_media::*;
pub use multi_channel_outbound::*;
pub use multi_channel_policy::*;
pub use multi_channel_routing::*;
pub use multi_channel_runtime::*;
pub use multi_channel_send::*;
