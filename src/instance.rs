//! Profile/instance management: per-install configs, mods directory, saves, etc.

use std::path::PathBuf;

pub struct Instance {
    pub name: String,
    pub version_id: String,
    pub dir: PathBuf,
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
