//! Wrapper calls around [`lirclone`]

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::ffi::{CStr, c_char};

use crate::errors::GalionError;

/// See the <https://github.com/rclone/rclone/tree/master/librclone> for details.
mod librclone_bindings {
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]

    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

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
            unsafe { librclone_bindings::RcloneInitialize() };
            self.librclone_is_initialized = true
        }
    }

    /// finalize lib
    pub fn finalize(&mut self) {
        if self.librclone_is_initialized {
            unsafe { librclone_bindings::RcloneFinalize() }
            self.librclone_is_initialized = false
        }
    }

    /// RPC call
    /// # Errors
    /// Errors if RPC call fails
    pub fn rpc(&self, method: &str, input: Value) -> Result<String, String> {
        let method_bytes = method.as_bytes();
        let mut method_c_chars: Vec<c_char> = method_bytes
            .iter()
            .map(|c| *c as c_char)
            .collect::<Vec<c_char>>();
        method_c_chars.push(0); // null terminator
        let method_mut_ptr: *mut c_char = method_c_chars.as_mut_ptr();

        let input_bytes: Vec<u8> = input.to_string().into_bytes();
        let mut input_c_chars: Vec<c_char> = input_bytes
            .iter()
            .map(|c| *c as c_char)
            .collect::<Vec<c_char>>();
        input_c_chars.push(0); // null terminator
        let input_mut_ptr: *mut c_char = input_c_chars.as_mut_ptr();

        let result = unsafe { librclone_bindings::RcloneRPC(method_mut_ptr, input_mut_ptr) };
        let output_c_str: &CStr = unsafe { CStr::from_ptr(result.Output) };
        let output_slice: &str = output_c_str
            .to_str()
            .map_err(|e| format!("Error formatting: {e}"))?;
        let output: String = output_slice.to_owned();
        unsafe { librclone_bindings::RcloneFreeString(result.Output) };

        match result.Status {
            200 => Ok(output),
            _ => Err(output),
        }
    }

    /// rclone noop test
    /// # Errors
    /// Fails if error with lib
    pub fn rc_noop(&self, value: Value) -> Result<Value, GalionError> {
        let res = self.rpc("rc/noop", value)?;
        let value = serde_json::from_str::<Value>(&res)?;
        Ok(value)
    }

    /// Get the rpc config
    /// # Errors
    /// Fails if error with lib
    pub fn get_rpc_config(&self) -> Result<Value, GalionError> {
        let res = self.rpc("options/get", json!({}))?;
        let value = serde_json::from_str::<Value>(&res)?;
        Ok(value)
    }

    /// Set the rpc config
    /// # Errors
    /// Fails if error with lib
    pub fn set_config_options(&self, conf: Value) -> Result<Value, GalionError> {
        let res = self.rpc("options/set", conf)?;
        let value = serde_json::from_str::<Value>(&res)?;
        Ok(value)
    }

    /// Set the rclone config path
    /// # Errors
    /// Fails if error with lib
    pub fn set_config_path(&self, config_path: &str) -> Result<Value, GalionError> {
        let input_json = json!({
            "path": config_path
        });
        let res = self.rpc("config/setpath", input_json)?;
        let value = serde_json::from_str::<Value>(&res)?;
        Ok(value)
    }

    /// Dump the rclone config
    /// # Errors
    /// Fails if error with lib
    pub fn dump_config(&self) -> Result<Value, GalionError> {
        let res = self.rpc("config/dump", json!({}))?;
        let value = serde_json::from_str::<Value>(&res)?;
        Ok(value)
    }

    /// List the remotes
    /// # Errors
    /// Fails if error with lib
    pub fn list_remotes(&self) -> Result<Vec<String>, GalionError> {
        let res = self.rpc("config/listremotes", json!({}))?;
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

    /// Get on remote
    /// # Errors
    /// Fails if error with lib
    pub fn get_remote(&self, remote_name: &str) -> Result<String, GalionError> {
        let res = self.rpc("config/get", json!({"name": remote_name}))?;
        // let value = serde_json::from_str::<Value>(&res)?;
        Ok(res)
    }

    /// Trigger a sync job
    /// # Errors
    /// Fails if error with lib
    pub fn sync(
        &self,
        src_fs: String,
        dest_fs: String,
        is_async: bool,
    ) -> Result<Value, GalionError> {
        match self.rpc(
            "sync/sync",
            json!({
                "srcFs": src_fs,
                "dstFs": dest_fs,
                "_async": is_async,
            }),
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
    pub fn job_list(&self) -> Result<RcJobList, GalionError> {
        let res = self.rpc("job/list", json!({}))?;
        let list = serde_json::from_str::<RcJobList>(&res)?;
        Ok(list)
    }

    /// Get job status by id
    /// # Errors
    /// Fails if error with lib
    pub fn job_status(&self, job_id: u64) -> Result<Value, GalionError> {
        let res = self.rpc("job/status", json!({ "jobid": job_id }))?;
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
    /// running ids
    #[serde(rename = "runningIds")]
    pub running_ids: Vec<u64>,
    /// finished ids
    #[serde(rename = "finishedIds")]
    pub finished_ids: Vec<u64>,
}
