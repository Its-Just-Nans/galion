//! Galion

#![warn(clippy::all, rust_2018_idioms)]
#![deny(
    missing_docs,
    clippy::all,
    clippy::missing_docs_in_private_items,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::cargo,
    clippy::unwrap_used,
    clippy::expect_used
)]
#![warn(clippy::multiple_crate_versions)]

mod app;
mod errors;
pub mod librclone;
mod remote;
mod ui;

pub use app::GalionApp;
pub use errors::GalionError;

/// Main galion CLI
/// # Errors
/// Fails if an error happens
pub fn galion_main() -> Result<(), GalionError> {
    let mut app = GalionApp::try_new_init()?;
    app.run_tui()?;
    app.quit()?;
    Ok(())
}
