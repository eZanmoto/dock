// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::ffi::OsStr;
use std::fmt::Debug;
use std::io::Error as IoError;
use std::process::ExitStatus;
use std::process::Stdio;

use snafu::OptionExt;
use snafu::ResultExt;
use snafu::Snafu;
use tokio::io::AsyncReadExt;
use tokio::process::Command as TokioCommand;

pub trait CommandLogger {
    fn log(&mut self, msg: CmdLoggerMsg);
}

pub enum CmdLoggerMsg<'a> {
    Cmd(&'a [&'a OsStr]),
    Start,
    StdoutWrite(&'a [u8]),
    StderrWrite(&'a [u8]),
    Exit,
}

#[tokio::main(flavor = "current_thread")]
pub async fn run(
    logger: &mut dyn CommandLogger,
    prog: &OsStr,
    args: &[&OsStr],
    stdin: Stdio,
)
    -> Result<ExitStatus, RunError>
{
    let mut cmd_line = vec![prog];
    cmd_line.extend(args);
    logger.log(CmdLoggerMsg::Cmd(&cmd_line));

    let mut cmd = TokioCommand::new(prog);

    cmd
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(stdin);

    logger.log(CmdLoggerMsg::Start);

    let mut child = cmd.spawn()
        .context(SpawnFailed)?;

    let mut stdout = child.stdout.take()
        .context(BindStdoutFailed)?;

    let mut stderr = child.stderr.take()
        .context(BindStderrFailed)?;

    let mut stdout_buf = [0; 0x1000];
    let mut stderr_buf = [0; 0x1000];

    let mut wait_status = None;
    while wait_status.is_none() {
        tokio::select! {
            result = stdout.read(&mut stdout_buf) => {
                let n = result
                    .context(ReadStdoutFailed)?;

                logger.log(CmdLoggerMsg::StdoutWrite(&stdout_buf[..n]));
            },

            result = stderr.read(&mut stderr_buf) => {
                let n = result
                    .context(ReadStderrFailed)?;

                logger.log(CmdLoggerMsg::StderrWrite(&stderr_buf[..n]));
            },

            result = child.wait() => {
                let status = result
                    .context(WaitFailed)?;

                logger.log(CmdLoggerMsg::Exit);

                wait_status = Some(status);
            },
        }
    }

    // `unwrap` is safe here because we assert that `wait_status` is not `None`
    // via the exit condition of `while`.
    Ok(wait_status.unwrap())
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
pub enum RunError {
    #[snafu(display("Couldn't spawn the command: {}", source))]
    SpawnFailed{source: IoError},
    #[snafu(display("Couldn't read the command's STDOUT: {}", source))]
    ReadStdoutFailed{source: IoError},
    #[snafu(display("Couldn't read the command's STDERR: {}", source))]
    ReadStderrFailed{source: IoError},
    #[snafu(display("Couldn't wait for the command: {}", source))]
    WaitFailed{source: IoError},

    // NOTE The following are considered "developer errors" - they aren't
    // expected to happen, and if they do, then this may indicate that tighter
    // handling needs to be performed when spawning the process.
    #[snafu(display("(Dev Err) Couldn't bind to the command STDOUT"))]
    BindStdoutFailed,
    #[snafu(display("(Dev Err) Couldn't bind to the command STDERR"))]
    BindStderrFailed,
}
