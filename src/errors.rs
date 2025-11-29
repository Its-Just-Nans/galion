//! Galion errors

use serde_json::Value;
use std::{fmt, io, sync::Arc};
use tokio::task::JoinError;

/// Galion error wrapper
#[derive(Debug)]
pub struct GalionError {
    /// error message
    pub message: String,
    /// source error
    pub source: Option<Arc<dyn std::error::Error + Send + Sync>>,
}

impl Clone for GalionError {
    fn clone(&self) -> Self {
        Self {
            message: self.message.clone(),
            source: self.source.clone(),
        }
    }
}
impl fmt::Display for GalionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.source {
            Some(src) => write!(f, "{} - caused by: {}", self.message, src),
            None => write!(f, "{}", self.message),
        }
    }
}

impl GalionError {
    /// Create new AppError
    pub fn new<S: AsRef<str>>(s: S) -> Self {
        let ref_str = s.as_ref();
        let message = ref_str.to_string();
        Self {
            message,
            source: None,
        }
    }
}

impl From<&str> for GalionError {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

impl From<String> for GalionError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<io::Error> for GalionError {
    fn from(error: io::Error) -> Self {
        Self {
            message: error.to_string(),
            source: Some(Arc::new(error)),
        }
    }
}

impl From<serde_json::Error> for GalionError {
    fn from(error: serde_json::Error) -> Self {
        Self {
            message: error.to_string(),
            source: Some(Arc::new(error)),
        }
    }
}

impl From<clap::error::Error> for GalionError {
    fn from(error: clap::error::Error) -> Self {
        Self {
            message: error.to_string(),
            source: Some(Arc::new(error)),
        }
    }
}

impl From<Value> for GalionError {
    fn from(value: Value) -> Self {
        match value.get("error") {
            Some(Value::String(error_message)) => Self::new(error_message.clone()),
            _ => Self::new(value.to_string()),
        }
    }
}

impl From<JoinError> for GalionError {
    fn from(value: JoinError) -> Self {
        Self {
            message: value.to_string(),
            source: Some(Arc::new(value)),
        }
    }
}

// #[derive(Debug, Default)]
// pub struct ErrorManager {
//     pub errors: Vec<AppError>,
// }

// impl ErrorManager {
//     pub fn new() -> Self {
//         Self {
//             ..Default::default()
//         }
//     }

//     pub fn add_error(&mut self, error: AppError) {
//         self.errors.push(error);
//     }

//     pub fn handle_error<T>(&mut self, error: Result<T, impl Into<AppError>>) -> Option<T> {
//         match error {
//             Ok(value) => Some(value),
//             Err(e) => {
//                 self.add_error(e.into());
//                 None
//             }
//         }
//     }
// }
