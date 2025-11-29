//! galion main app

use clap::ArgAction;
use clap::Parser;
use home::home_dir;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use crate::errors::GalionError;
use crate::librclone::rclone::Rclone;
use crate::remote::ConfigOrigin;
use crate::remote::RemoteConfiguration;

/// Galion ASCII art
/// This ASCII pic can be found at https://asciiart.website/art/4370
pub(crate) const GALION_ASCII_ART: &str = r#"
     _~
  _~ )_)_~
  )_))_))_)
  _!__!__!_
  \______t/
~~~~~~~~~~~~~"#;

/// remote configuration
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AppConfiguration {
    /// list of remote configuration
    remote_configurations: Vec<RemoteConfiguration>,
}

/// Galion arguments parsing
#[derive(Parser, Debug)]
#[command(name = "galion", version, about = "Galion CLI")]
pub struct GalionArgs {
    /// Path to the configuration file
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Full path to the configuration file
    #[arg(long, value_name = "FILE")]
    rclone_config: Option<PathBuf>,

    /// Full path to the configuration file
    #[arg(long)]
    rclone_ask_password: bool,

    /// Full path to the configuration file
    #[arg(long, action=ArgAction::SetTrue)]
    hide_banner: bool,

    /// Should update the config file
    #[arg(long, action=ArgAction::SetTrue)]
    update_config: bool,
}

/// Galion App
#[derive(Debug)]
pub struct GalionApp {
    /// config path
    config_path: PathBuf,
    /// args
    galion_args: GalionArgs,
    /// config
    config: AppConfiguration,
    /// rclone instance
    pub rclone: Arc<Mutex<Rclone>>,
}

/// app name
const APP_NAME: &str = "galion";

impl GalionApp {
    /// Create new galion instance
    /// # Errors
    /// Error if fails
    pub fn try_new() -> Result<Self, GalionError> {
        let galion_args = GalionArgs::try_parse()?;
        let config_path = galion_args
            .config
            .clone()
            .unwrap_or(Self::get_default_config_path()?);
        let config = Self::load_config(&config_path)?;
        Ok(Self {
            config,
            galion_args,
            rclone: Default::default(),
            config_path,
        })
    }

    /// Create new galion instance and init it
    /// # Errors
    /// Error if fails
    pub fn try_new_init() -> Result<Self, GalionError> {
        let mut galion = Self::try_new()?;
        {
            let mut rclone = galion
                .rclone
                .lock()
                .map_err(|e| GalionError::new(e.to_string()))?;
            rclone.initialize();
        }
        galion.init()?;
        Ok(galion)
    }

    /// Get a remote configuration
    pub fn get_remote_config(&self, remote_name: &str) -> Option<&RemoteConfiguration> {
        self.config
            .remote_configurations
            .iter()
            .find(|r| r.remote_name == remote_name)
    }

    /// Get the config path
    /// # Errors
    /// Fails if home_dir not found
    pub fn get_default_config_path() -> Result<PathBuf, GalionError> {
        let mut path = home_dir().ok_or("Unable to get home directory")?;
        path.push(".config");
        path.push(APP_NAME);
        path.push("galion.json");
        Ok(path)
    }

    /// Load the config
    /// # Errors
    /// Fails if fails to log the config
    fn load_config(config_path: &Path) -> Result<AppConfiguration, GalionError> {
        if !config_path.exists() {
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let config_json = serde_json::to_string(&AppConfiguration::default())?;
            std::fs::write(config_path, config_json)?;
        }
        let config_data = std::fs::read_to_string(config_path)?;
        let loaded_config = serde_json::from_str(&config_data)?;
        Ok(loaded_config)
    }

    /// Init the app
    /// # Errors
    /// Fails if fails to init
    pub fn init(&mut self) -> Result<(), GalionError> {
        let rclone = self
            .rclone
            .lock()
            .map_err(|e| GalionError::new(e.to_string()))?;
        if let Some(rclone_config_path) = &self.galion_args.rclone_config {
            rclone.set_config_path(&rclone_config_path.to_string_lossy())?;
        }
        if !self.galion_args.hide_banner {
            println!("{}", GALION_ASCII_ART);
        }
        rclone.set_config_options(json!({
            "main": {
                "LogLevel": "CRITICAL",
            },
        }))?;
        if !self.galion_args.rclone_ask_password {
            rclone.set_config_options(json!({
                "main": {
                    "AskPassword": false,
                },
            }))?;
        }
        if let Err(e) = rclone.dump_config() {
            let msg = if self.galion_args.rclone_ask_password {
                " and the decryption failed"
            } else {
                "and you can retry with the --rclone-ask-password flag"
            };
            return Err(GalionError::new(format!(
                "Failed to get the rclone configuration. Most likely the configuration is encrypted {} - {}",
                msg, e
            )));
        }
        let list_remotes = rclone.list_remotes()?;
        for remote in list_remotes {
            if self
                .config
                .remote_configurations
                .iter()
                .any(|r| r.remote_name == remote)
            {
                continue;
            }
            let remote_conf = rclone.get_remote(&remote)?;
            println!("{}", remote_conf);
            let remote_config = RemoteConfiguration {
                remote_name: remote,
                local_path: None,
                remote_path: None,
                config_origin: ConfigOrigin::RcloneConfig,
            };
            self.config.remote_configurations.push(remote_config);
        }
        if self.galion_args.update_config {
            std::fs::write(&self.config_path, serde_json::to_string(&self.config)?)?;
        }
        if self.config.remote_configurations.is_empty() {
            return Err(GalionError::new(format!(
                "No remote found in rclone 'config/listremotes' and in the galion config at {} - please add remote with rclone CLI",
                self.config_path.display()
            )));
        }

        Ok(())
    }

    /// Returns the remotes
    pub fn remotes(&self) -> Vec<RemoteConfiguration> {
        self.config.remote_configurations.clone()
    }

    /// Quit app
    /// # Errors
    /// Fails if save config fails or librclone fails
    pub fn quit(&mut self) -> Result<(), GalionError> {
        {
            let mut rclone = self
                .rclone
                .lock()
                .map_err(|e| GalionError::new(e.to_string()))?;
            rclone.finalize();
        }
        Ok(())
    }
}

impl Drop for GalionApp {
    fn drop(&mut self) {
        let mut rclone = self.rclone.lock().unwrap();
        rclone.finalize();
    }
}
