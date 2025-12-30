//! Remote configuration

use std::fmt::Display;

use serde::{Deserialize, Serialize};

/// Config origin
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
pub enum ConfigOrigin {
    /// from galion config
    #[default]
    GalionConfig,
    /// from rclone config
    RcloneConfig,
}

impl Display for ConfigOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GalionConfig => write!(f, "galion config"),
            Self::RcloneConfig => write!(f, "rclone config"),
        }
    }
}

/// Remote Configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemoteConfiguration {
    /// remote name in the config
    pub remote_name: String,
    /// local path
    pub remote_src: Option<String>,
    /// remote path
    pub remote_dest: Option<String>,

    /// config origin
    #[serde(skip)]
    pub config_origin: ConfigOrigin,
}

impl RemoteConfiguration {
    /// Translate to a row
    pub fn to_table_row(&self) -> [String; 3] {
        [
            format!("{}\n{}", self.remote_name, self.config_origin),
            self.remote_src.clone().unwrap_or_default(),
            self.remote_dest.clone().unwrap_or_default(),
        ]
    }
}
