//! Multi-channel transport runtime building blocks for Tau.
//!
//! Provides connector, routing, lifecycle, ingress, outbound, policy, and
//! telemetry components for Telegram/Discord/WhatsApp-style channels.
//!
//! Architecture reference:
//! - [`docs/guides/multi-channel-event-pipeline.md`](../../../docs/guides/multi-channel-event-pipeline.md)
//!
//! ```rust
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use tau_multi_channel::parse_multi_channel_live_inbound_envelope;
//!
//! let raw = r#"{
//!   "schema_version": 1,
//!   "transport": "telegram",
//!   "provider": "telegram-bot-api",
//!   "payload": {
//!     "update_id": 42,
//!     "message": {
//!       "message_id": 7,
//!       "date": 1700000000,
//!       "text": "hello",
//!       "chat": { "id": "chat-1", "type": "private" },
//!       "from": { "id": "user-1", "username": "operator" }
//!     }
//!   }
//! }"#;
//!
//! let event = parse_multi_channel_live_inbound_envelope(raw)?;
//! assert_eq!(event.transport.as_str(), "telegram");
//! assert_eq!(event.event_id, "7");
//! # Ok(())
//! # }
//! ```

pub mod multi_channel_contract;
pub mod multi_channel_credentials;
pub mod multi_channel_incident;
pub mod multi_channel_lifecycle;
pub mod multi_channel_live_connectors;
pub mod multi_channel_live_ingress;
pub mod multi_channel_media;
pub mod multi_channel_outbound;
pub mod multi_channel_policy;
pub mod multi_channel_route_inspect;
pub mod multi_channel_routing;
pub mod multi_channel_runtime;
pub mod multi_channel_send;

pub use multi_channel_contract::*;
pub use multi_channel_credentials::*;
pub use multi_channel_incident::*;
pub use multi_channel_lifecycle::*;
pub use multi_channel_live_connectors::*;
pub use multi_channel_live_ingress::*;
pub use multi_channel_media::*;
pub use multi_channel_outbound::*;
pub use multi_channel_policy::*;
pub use multi_channel_route_inspect::*;
pub use multi_channel_routing::*;
pub use multi_channel_runtime::*;
pub use multi_channel_send::*;
