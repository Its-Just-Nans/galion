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

/// Input string state
#[derive(Debug)]
pub(crate) struct EditRemote {
    /// idx edit string
    pub(crate) idx_string: usize,
    /// Position of cursor in the editor area
    pub(crate) character_index: usize,
    /// Remote name
    pub(crate) edit_remote_name: String,
    /// Remote src
    pub(crate) edit_remote_src: String,
    /// Remote destination
    pub(crate) edit_remote_dest: String,
}

impl EditRemote {
    /// Byte index of the selected input
    fn byte_index(&mut self) -> usize {
        let input = match self.idx_string {
            0 => &mut self.edit_remote_name,
            1 => &mut self.edit_remote_src,
            _ => &mut self.edit_remote_dest,
        };
        input
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.character_index)
            .unwrap_or(input.len())
    }

    /// Add a char to a selected input
    pub fn enter_char(&mut self, new_char: char) {
        let index = self.byte_index();
        let input = match self.idx_string {
            0 => &mut self.edit_remote_name,
            1 => &mut self.edit_remote_src,
            _ => &mut self.edit_remote_dest,
        };
        input.insert(index, new_char);
        self.move_cursor_right();
    }

    /// Clamp cursor based on the selected input
    fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        let input_count = match self.idx_string {
            0 => self.edit_remote_name.chars().count(),
            1 => self.edit_remote_src.chars().count(),
            _ => self.edit_remote_dest.chars().count(),
        };
        new_cursor_pos.clamp(0, input_count)
    }

    /// Move the cursor to the right for the selected input
    pub fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.character_index.saturating_add(1);
        self.character_index = self.clamp_cursor(cursor_moved_right);
    }

    /// Move the cursor to the left for the selected input
    pub fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.character_index.saturating_sub(1);
        self.character_index = self.clamp_cursor(cursor_moved_left);
    }

    /// Delete char for the selected input
    pub fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.character_index != 0;
        if is_not_cursor_leftmost {
            let input = match self.idx_string {
                0 => &mut self.edit_remote_name,
                1 => &mut self.edit_remote_src,
                _ => &mut self.edit_remote_dest,
            };
            // Method "remove" is not used on the saved text for deleting the selected char.
            // Reason: Using remove on String works on bytes instead of the chars.
            // Using remove would require special care because of char boundaries.

            let current_index = self.character_index;
            let from_left_to_current_index = current_index - 1;

            // Getting all characters before the selected character.
            let before_char_to_delete = input.chars().take(from_left_to_current_index);
            // Getting all characters after selected character.
            let after_char_to_delete = input.chars().skip(current_index);

            // Put all characters together except the selected one.
            // By leaving the selected one out, it is forgotten and therefore deleted.
            *input = before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_cursor_left();
        }
    }

    /// Reset char index
    pub fn reset_char_index(&mut self) {
        let input_len = match self.idx_string {
            0 => self.edit_remote_name.chars().count(),
            1 => self.edit_remote_src.chars().count(),
            _ => self.edit_remote_dest.chars().count(),
        };
        self.character_index = self.clamp_cursor(input_len);
    }

    /// Get the edited new remote
    pub fn finish(&self) -> RemoteConfiguration {
        RemoteConfiguration {
            remote_name: self.edit_remote_name.clone(),
            remote_src: Some(self.edit_remote_src.clone()),
            remote_dest: Some(self.edit_remote_dest.clone()),
            config_origin: ConfigOrigin::GalionConfig,
        }
    }
}
