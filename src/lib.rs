pub mod capture;
pub mod pairing;
pub mod server;
pub mod settings;
pub mod ui;
pub mod update;
#[cfg(feature = "virtual-display")]
pub mod virtual_display;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
