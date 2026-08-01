//! External integrations: Discord Rich Presence and the MPRIS2 D-Bus service.

#[cfg(feature = "discord")]
pub mod discord;
#[cfg(feature = "mpris")]
pub mod mpris;

#[cfg(feature = "discord")]
pub use discord::Discord;
#[cfg(feature = "mpris")]
pub use mpris::run as run_mpris;
