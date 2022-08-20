// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::ffi::OsStr;
use std::fmt::Debug;
use std::io::Error as IoError;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Output;
use std::process::Stdio;

use snafu::ResultExt;
use snafu::Snafu;

pub fn assert_run<I, S>(args: I) -> Result<Output, AssertRunError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output =
        Command::new("docker")
            .args(args)
            .stdin(Stdio::null())
            .output()
            .context(RunFailed)?;

    if !output.status.success() {
        return Err(AssertRunError::NonZeroExit{output});
    }

    Ok(output)
}

#[derive(Debug, Snafu)]
pub enum AssertRunError {
    #[snafu(display("Couldn't run a new `docker` process: {}", source))]
    RunFailed{source: IoError},
    #[snafu(display("The `docker` process returned non-zero: {:?}", output))]
    NonZeroExit{output: Output},
}

// `stream_run` runs a `docker` subcommand but passes the file descriptors
// for the standard streams of the current process to the child, so all input
// and output will be passed as if the subcommand was the current process.
pub fn stream_run<I, S>(args: I) -> Result<ExitStatus, StreamRunError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    // The process spawned by `Command` inherits the standard file descriptors
    // from the parent process by default.
    let mut child =
        Command::new("docker")
            .args(args)
            .spawn()
            .context(SpawnFailed)?;

    let status = child.wait()
        .context(WaitFailed)?;

    Ok(status)
}

#[derive(Debug, Snafu)]
pub enum StreamRunError {
    #[snafu(display("Couldn't spawn a new `docker` process: {}", source))]
    SpawnFailed{source: IoError},
    #[snafu(display("Couldn't wait for the `docker` process: {}", source))]
    WaitFailed{source: IoError},
}
