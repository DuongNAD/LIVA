//! Multi-Channel Adapter Architecture (Milestone 2: Features 5-9)
//!
//! Provides the unified multi-channel messaging adapter ecosystem:
//! - `adapter`: Core `ChannelAdapter` trait, capabilities matrix, lifecycle, status, error, and backoff calculator.
//! - `telegram`: Upgraded Telegram bot adapter with Teloxide, streaming debounce, and voice PTT.
//! - `whatsapp`: WhatsApp Meta Cloud API webhook adapter with HMAC-SHA256 signature verification.
//! - `discord`: Discord Gateway WebSocket & REST API adapter with markdown & thread support.
//! - `slack`: Slack Socket Mode & Web API adapter with Block Kit & thread timestamps (`thread_ts`).

pub mod adapter;
pub mod discord;
pub mod slack;
pub mod telegram;
pub mod whatsapp;

pub use adapter::{
    BackoffCalculator, ChannelAdapter, ChannelCapabilities, ChannelError, ChannelStatus,
    ExponentialBackoff, StreamOptions,
};
pub use discord::{DiscordAdapter, DiscordConfig};
pub use slack::{SlackAdapter, SlackConfig};
pub use telegram::{TelegramAdapter, TelegramConfig};
pub use whatsapp::{WhatsAppAdapter, WhatsAppConfig};
