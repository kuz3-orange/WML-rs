//! Profile/instance management: per-install configs, mods directory, saves, etc.

use std::path::PathBuf;

use crate::java::JavaRuntime;

pub struct Instance {
    pub name: String,
    pub version_id: String,
    pub dir: PathBuf,
    /// UUID of the account this instance launches as. Each instance picks
    /// its own account rather than sharing one global sign-in.
    pub account_uuid: String,
    /// Java runtime this instance launches with. Managed (downloaded and
    /// pinned by the launcher) rather than just detected from the system.
    pub java: JavaRuntime,
}

pub fn list_instances() -> Vec<Instance> {
    todo!("enumerate instances under the instances directory")
}

pub fn create_instance(name: &str, version_id: &str) -> Result<Instance, InstanceError> {
    todo!("scaffold a new instance directory for the given version")
}

pub fn delete_instance(instance: &Instance) -> Result<(), InstanceError> {
    todo!("remove an instance's directory")
}

pub enum InstanceError {
    AlreadyExists,
    NotFound,
    IoError,
}
