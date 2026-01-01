//! Galion
//!
//! ```txt
//!     _~
//!  _~ )_)_~
//!  )_))_))_)     galion
//!  _!__!__!_     sync tui for rclone
//!  \______t/
//! ```
//!
//! # Usage
//! ```
//! cargo install galion --locked
//! galion -h
//! ```

#![warn(clippy::all, rust_2018_idioms)]
#![deny(
    missing_docs,
    clippy::all,
    clippy::missing_docs_in_private_items,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::cargo,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::pedantic
)]
#![warn(clippy::multiple_crate_versions)]

mod app;
mod errors;
pub mod librclone;
mod remote;
mod ui;

pub use app::GalionApp;
pub use app::GalionArgs;
pub use errors::GalionError;

/// Main galion CLI
/// # Errors
/// Fails if an error happens
pub fn galion_main() -> Result<(), GalionError> {
    use clap::Parser;
    let args: Vec<String> = std::env::args().collect();
    let galion_args =
        GalionArgs::try_parse_from(args).map_err(|e| e.to_string().trim_end().to_string())?;
    let app = GalionApp::try_from_galion_args(galion_args)?;
    app.run_tui()?;
    Ok(())
}
