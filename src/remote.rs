//! Remote configuration

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{GalionError, librclone::rclone::Rclone};

/// Config origin
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub enum ConfigOrigin {
    /// from galion config
    #[default]
    GalionConfig,
    /// from rclone config
    RcloneConfig,
}

/// Remote Configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemoteConfiguration {
    /// local path
    pub local_path: Option<String>,
    /// remote name
    pub remote_name: String,
    /// remote path
    pub remote_path: Option<String>,

    /// config origin
    #[serde(skip)]
    pub config_origin: ConfigOrigin,
}

impl RemoteConfiguration {
    /// Sync a remote
    /// # Errors
    /// Errors if fails to send remote
    pub fn sync(self, rclone: Rclone) -> Result<Value, GalionError> {
        if let Some(local_path) = &self.local_path {
            if let Some(_remote_path) = &self.remote_path {
                let dest = self.get_destination();
                rclone.sync(local_path.clone(), dest, true)
            } else {
                Err(GalionError::new("Remote path is not set"))
            }
        } else {
            Err(GalionError::new("Local path is not set"))
        }
    }

    /// Get the destination
    pub fn get_destination(&self) -> String {
        format!(
            "{}:{}",
            self.remote_name,
            self.remote_path.as_deref().unwrap_or("")
        )
    }

    /// Translate to a row
    pub fn to_table_row(&self) -> [String; 3] {
        [
            self.remote_name.clone(),
            self.local_path.clone().unwrap_or_default(),
            self.get_destination(),
        ]
    }
}
