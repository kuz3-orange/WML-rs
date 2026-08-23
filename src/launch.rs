//! Building the game launch command and spawning the Minecraft process.

use crate::auth::Account;
use crate::instance::Instance;
use crate::java::JavaRuntime;

pub struct LaunchOptions<'a> {
    pub instance: &'a Instance,
    pub account: &'a Account,
    pub java: &'a JavaRuntime,
}

pub fn build_command(options: &LaunchOptions) -> Vec<String> {
    todo!("assemble the JVM args, classpath, and game args for this instance")
}

pub fn launch(options: &LaunchOptions) -> Result<std::process::Child, LaunchError> {
    todo!("spawn the game process with the built command")
}

pub enum LaunchError {
    MissingAssets,
    ProcessSpawnFailed,
}
