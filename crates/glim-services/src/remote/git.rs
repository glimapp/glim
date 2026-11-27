use super::Session;
use anyhow::Result;
use std::{ffi::OsString, os::unix::ffi::OsStrExt, process::Command};

impl Session {
    pub(crate) fn git_command(&self, arguments: &[OsString]) -> Result<Command> {
        let mut script = b"LC_ALL=C GIT_TERMINAL_PROMPT=0 GIT_EDITOR=true GIT_SEQUENCE_EDITOR=true GIT_OPTIONAL_LOCKS=0 git".to_vec();
        for argument in arguments {
            script.push(b' ');
            super::quote_shell_word(argument.as_bytes(), &mut script);
        }
        self.connection.batch_shell_command(script)
    }
}
