//! Injectable command execution.

use std::process::Command;
use thiserror::Error;

#[cfg(test)]
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("command unavailable: {0}")]
    Unavailable(String),
    #[error("command execution failed: {0}")]
    Io(#[from] std::io::Error),
}

pub trait CommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, CommandError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, CommandError> {
        let output = Command::new(program).args(args).output().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                CommandError::Unavailable(program.to_string())
            } else {
                CommandError::Io(error)
            }
        })?;
        Ok(CommandOutput {
            status: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// FIFO scripted runner for tests. Missing commands are Unavailable unless `always_ok`.
#[cfg(test)]
#[derive(Debug)]
pub struct ScriptedRunner {
    pub always_ok: bool,
    pub calls: std::sync::Mutex<Vec<(String, Vec<String>)>>,
    replies: std::sync::Mutex<VecDeque<(String, Result<CommandOutput, CommandError>)>>,
}

#[cfg(test)]
impl Default for ScriptedRunner {
    fn default() -> Self {
        Self {
            always_ok: false,
            calls: std::sync::Mutex::new(Vec::new()),
            replies: std::sync::Mutex::new(VecDeque::new()),
        }
    }
}

#[cfg(test)]
impl ScriptedRunner {
    pub fn succeeding() -> Self {
        Self {
            always_ok: true,
            ..Self::default()
        }
    }

    #[allow(dead_code)]
    pub fn push_unavailable(&self, program: &str) {
        self.replies
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push_back((
                program.into(),
                Err(CommandError::Unavailable(program.into())),
            ));
    }

    #[allow(dead_code)]
    pub fn push_ok(&self, program: &str, stdout: &str) {
        self.replies
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push_back((
                program.into(),
                Ok(CommandOutput {
                    status: Some(0),
                    stdout: stdout.into(),
                    stderr: String::new(),
                }),
            ));
    }
}

#[cfg(test)]
impl Clone for ScriptedRunner {
    fn clone(&self) -> Self {
        Self::default()
    }
}

#[cfg(test)]
impl CommandRunner for ScriptedRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, CommandError> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).push((
            program.into(),
            args.iter().map(|s| (*s).to_string()).collect(),
        ));
        let mut replies = self.replies.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((expected, result)) = replies.pop_front() {
            if expected == program {
                return result;
            }
            replies.push_front((expected, result));
        }
        if self.always_ok {
            let status = match program {
                "nft" if args == ["list", "table", "inet", "router"] => Some(1),
                "systemctl" if args == ["is-enabled", "gateway-firewall.service"] => Some(1),
                "test" => Some(1),
                _ => Some(0),
            };
            let stdout = match program {
                "nft" => "table inet gateway_kit",
                "ip" if args == ["rule", "show"] => "fwmark 1 lookup 51820",
                "ip" if args.starts_with(&["route", "show"]) => "local 0.0.0.0/0 dev lo scope host",
                "ip" if args.starts_with(&["-o", "link"]) => "wg0: <POINTOPOINT,UP>",
                "ss" => "tcp LISTEN 127.0.0.1:7895\nudp UNCONN 127.0.0.1:5353",
                "wg" => "wg0\tkey=\t1",
                _ => "",
            };
            return Ok(CommandOutput {
                status,
                stdout: stdout.into(),
                stderr: String::new(),
            });
        }
        Err(CommandError::Unavailable(program.into()))
    }
}
