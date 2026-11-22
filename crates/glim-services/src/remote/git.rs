use super::Session;
use anyhow::Result;
use std::{ffi::OsString, process::Command};

impl Session {
    pub(crate) fn git_command(&self, arguments: &[OsString]) -> Result<Command> {
        let mut script = b"LC_ALL=C GIT_TERMINAL_PROMPT=0 GIT_EDITOR=true GIT_SEQUENCE_EDITOR=true GIT_OPTIONAL_LOCKS=0 git".to_vec();
        for argument in arguments {
            script.extend_from_slice(b" '");
            for byte in argument.as_encoded_bytes() {
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
            .arg(crate::platform::decode_path_bytes(script)?);
        Ok(command)
    }
}
