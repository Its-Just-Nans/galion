//! galion main app

use clap::ArgAction;
use clap::Parser;
use home::home_dir;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

use crate::errors::GalionError;
use crate::librclone::rclone::Rclone;
use crate::remote::ConfigOrigin;
use crate::remote::RemoteConfiguration;

/// remote configuration
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct GalionConfig {
    /// list of remote configuration
    pub(crate) remote_configurations: Vec<RemoteConfiguration>,

    /// Config path
    #[serde(skip)]
    pub(crate) config_path: PathBuf,
}

impl GalionConfig {
    /// Load the config
    /// # Errors
    /// Fails if fails to log the config
    fn load_config(config_path: PathBuf) -> Result<GalionConfig, GalionError> {
        if !config_path.exists() {
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let config_json = serde_json::to_string(&GalionConfig::default())?;
            std::fs::write(&config_path, config_json)?;
        }
        let config_data = std::fs::read_to_string(&config_path)?;
        let mut loaded_config = serde_json::from_str::<GalionConfig>(&config_data)?;
        loaded_config.config_path = config_path;
        Ok(loaded_config)
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

    /// Returns the remotes
    pub fn remotes(&self) -> &[RemoteConfiguration] {
        &self.remote_configurations
    }

    /// Save galion config
    /// # Errors
    /// Fails if write to file fails
    pub fn save_config(&self) -> Result<(), GalionError> {
        let remotes_to_save = self
            .remote_configurations
            .iter()
            .filter(|c| c.config_origin == ConfigOrigin::GalionConfig)
            .cloned()
            .collect::<Vec<RemoteConfiguration>>();
        let config = GalionConfig {
            remote_configurations: remotes_to_save,
            config_path: self.config_path.clone(),
        };
        std::fs::write(&self.config_path, serde_json::to_string(&config)?)?;
        Ok(())
    }
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
    pub(crate) hide_banner: bool,

    /// Should update the config file (false)
    #[arg(long, action=ArgAction::SetTrue)]
    auto_update_config: bool,

    /// Ignore fuplicate remote
    #[arg(long, action=ArgAction::SetTrue)]
    ignore_duplicate_remote: bool,
}

/// Galion App
#[derive(Debug)]
pub struct GalionApp {
    /// args
    pub(crate) galion_args: GalionArgs,
    /// config
    pub(crate) config: GalionConfig,
    /// rclone instance
    pub(crate) rclone: Rclone,
}

/// app name
const APP_NAME: &str = "galion";

impl GalionApp {
    /// Galion ASCII art
    /// This ASCII pic can be found at https://asciiart.website/art/4370
    const GALION: &str = r#"    _~
 _~ )_)_~
 )_))_))_)
 _!__!__!_
 \______t/"#;

    /// Waves ASCII art
    pub(crate) const WAVES: &str = "~~~~~~~~~~~~";

    /// Create new galion instance
    /// # Errors
    /// Error if fails
    pub fn try_new(args: &[String]) -> Result<Self, GalionError> {
        let galion_args = GalionArgs::try_parse_from(args).map_err(|e| e.to_string())?;
        let config_path = galion_args
            .config
            .clone()
            .unwrap_or(GalionConfig::get_default_config_path()?);
        let config = GalionConfig::load_config(config_path)?;
        Ok(Self {
            config,
            galion_args,
            rclone: Rclone::new(),
        })
    }

    /// Galion logo
    pub fn logo() -> String {
        format!("{}\n{}", Self::GALION, Self::WAVES)
    }

    /// Galion logo with random waves
    pub fn logo_random_waves() -> String {
        let mut rng = rand::rng();

        let roll: u32 = rng.random_range(0..=9);
        let mut chars: Vec<char> = Self::WAVES.chars().collect();
        let len = chars.len();
        if roll > 5 && len >= 3 {
            let idx = rng.random_range(2..len - 3);
            chars[idx] = '-';
            chars[idx + 1] = '=';
            chars[idx + 2] = '-';
        }

        let waves: String = chars.into_iter().collect();
        format!("{}\n{}", Self::GALION, waves)
    }

    /// Logo with waves
    pub fn logo_waves() -> String {
        format!("{}\n{}", Self::GALION, Self::WAVES)
    }

    /// Create new galion instance and init it
    /// # Errors
    /// Error if fails
    pub fn try_new_init(args: &[String]) -> Result<Self, GalionError> {
        let mut galion = Self::try_new(args)?;
        galion.init()?;
        Ok(galion)
    }

    /// Init the app
    /// # Errors
    /// Fails if fails to init
    pub fn init(&mut self) -> Result<(), GalionError> {
        if let Some(rclone_config_path) = &self.galion_args.rclone_config {
            self.rclone
                .set_config_path(&rclone_config_path.to_string_lossy())?;
        }
        if !self.galion_args.hide_banner {
            println!("{}", Self::logo());
        }
        self.rclone.set_config_options(json!({
            "main": {
                "LogLevel": "CRITICAL",
            },
        }))?;
        if !self.galion_args.rclone_ask_password {
            self.rclone.set_config_options(json!({
                "main": {
                    "AskPassword": false,
                },
            }))?;
        }
        if let Err(e) = self.rclone.dump_config() {
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
        let list_remotes = self.rclone.list_remotes()?;
        for rclone_remote_name in list_remotes {
            if self
                .config
                .remote_configurations
                .iter()
                .any(|r| r.remote_name == rclone_remote_name)
                && self.galion_args.ignore_duplicate_remote
            {
                continue;
            }
            let remote_conf = self.rclone.get_remote(&rclone_remote_name)?;
            let remote_dest = remote_conf
                .get("remote")
                .and_then(|v| v.as_str())
                .map(String::from);
            let remote_config = RemoteConfiguration {
                remote_name: rclone_remote_name,
                remote_src: None,
                remote_dest,
                config_origin: ConfigOrigin::RcloneConfig,
            };
            self.config.remote_configurations.push(remote_config);
        }
        if self.galion_args.auto_update_config {
            self.config.save_config()?;
        }
        if self.config.remote_configurations.is_empty() {
            return Err(GalionError::new(format!(
                "No remote found in rclone 'config/listremotes' and in the galion config at {} - please add remote with rclone CLI",
                self.config.config_path.display()
            )));
        }

        Ok(())
    }
}

impl Drop for GalionApp {
    fn drop(&mut self) {
        self.rclone.finalize();
    }
}
