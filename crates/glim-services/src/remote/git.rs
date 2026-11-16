use super::Session;
use anyhow::Result;
use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
    process::Command,
};

impl Session {
    pub(crate) fn git_command(&self, arguments: &[OsString]) -> Result<Command> {
        let mut script = b"GIT_OPTIONAL_LOCKS=0 git".to_vec();
        for argument in arguments {
            script.extend_from_slice(b" '");
            for byte in argument.as_bytes() {
                if *byte == b'\'' {
                    script.extend_from_slice(b"'\"'\"'");
                } else {
                    script.push(*byte);
                }
            }
            script.push(b'\'');
        }
        let mut command = self.connection.ssh_command()?;
        command
            .args(["-o", "BatchMode=yes"])
            .arg(&self.connection.host)
            .arg(OsString::from_vec(script));
        Ok(command)
    }
}
