//! Wrapper calls around [`lirclone`]

use librclone::{finalize as lib_finalize, initialize as lib_initialize, rpc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::errors::GalionError;

/// Rclone wrapper
#[derive(Debug, Default)]
pub struct Rclone {
    /// Is lib rclone init
    librclone_is_initialized: bool,
}

impl Rclone {
    /// initialize lib
    pub fn initialize(&mut self) {
        if !self.librclone_is_initialized {
            lib_initialize();
            self.librclone_is_initialized = true
        }
    }

    /// finalize lib
    pub fn finalize(&mut self) {
        if self.librclone_is_initialized {
            lib_finalize();
            self.librclone_is_initialized = false
        }
    }

    /// rclone noop test
    /// # Errors
    /// Fails if error with lib
    pub fn rc_noop(value: Value) -> Result<Value, GalionError> {
        let res = rpc("rc/noop", value.to_string())?;
        let value = serde_json::from_str::<Value>(&res)?;
        Ok(value)
    }

    /// Get the rpc config
    /// # Errors
    /// Fails if error with lib
    pub fn get_rpc_config() -> Result<Value, GalionError> {
        let res = rpc("options/get", json!({}).to_string())?;
        let value = serde_json::from_str::<Value>(&res)?;
        Ok(value)
    }

    /// Set the rpc config
    /// # Errors
    /// Fails if error with lib
    pub fn set_config_options(conf: Value) -> Result<Value, GalionError> {
        let res = rpc("options/set", conf.to_string())?;
        let value = serde_json::from_str::<Value>(&res)?;
        Ok(value)
    }

    /// Set the rclone config path
    /// # Errors
    /// Fails if error with lib
    pub fn set_config_path(config_path: &str) -> Result<Value, GalionError> {
        let input_json = json!({
            "path": config_path
        })
        .to_string();
        let res = rpc("config/setpath", input_json)?;
        let value = serde_json::from_str::<Value>(&res)?;
        Ok(value)
    }

    /// Dump the rclone config
    /// # Errors
    /// Fails if error with lib
    pub fn dump_config() -> Result<Value, GalionError> {
        let res = rpc("config/dump", json!({}).to_string())?;
        let value = serde_json::from_str::<Value>(&res)?;
        Ok(value)
    }

    /// List the remotes
    /// # Errors
    /// Fails if error with lib
    pub fn listremotes() -> Result<Vec<String>, GalionError> {
        let res = rpc("config/listremotes", json!({}).to_string())?;
        let value = serde_json::from_str::<Value>(&res)?;
        match value {
            Value::Object(arr) => match arr.get("remotes") {
                Some(Value::Array(remotes_list)) => {
                    let mut remotes = Vec::new();
                    for remote in remotes_list {
                        if let Value::String(remote_name) = remote {
                            remotes.push(remote_name.clone());
                        }
                    }
                    Ok(remotes)
                }
                _ => Ok(vec![]),
            },
            _ => Err("Bad response - no remotes".into()),
        }
    }

    /// Trigger a sync job
    /// # Errors
    /// Fails if error with lib
    pub fn sync(src_fs: String, dest_fs: String, is_async: bool) -> Result<Value, GalionError> {
        match rpc(
            "sync/sync",
            json!({
                "srcFs": src_fs,
                "dstFs": dest_fs,
                "_async": is_async,
            })
            .to_string(),
        ) {
            Ok(res) => {
                let value = serde_json::from_str::<Value>(&res)?;
                Ok(value)
            }
            Err(e) => {
                let value = serde_json::from_str::<Value>(&e)?;
                Err(value.into())
            }
        }
    }

    /// List rclone jobs
    /// # Errors
    /// Fails if error with lib
    pub fn job_list() -> Result<RcJobList, GalionError> {
        let res = rpc("job/list", json!({}).to_string())?;
        let list = serde_json::from_str::<RcJobList>(&res)?;
        Ok(list)
    }

    /// Get job status by id
    /// # Errors
    /// Fails if error with lib
    pub fn job_status(job_id: u64) -> Result<Value, GalionError> {
        let res = rpc("job/status", json!({ "jobid": job_id }).to_string())?;
        let value = serde_json::from_str::<Value>(&res)?;
        Ok(value)
    }
}

/// RcJobList
#[derive(Debug, Deserialize, Serialize)]
pub struct RcJobList {
    /// ids of jobs
    #[serde(rename = "jobids")]
    pub job_ids: Vec<u64>,
}
