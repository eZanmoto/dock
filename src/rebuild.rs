// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::error::Error;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::fs::File;
use std::io;
use std::io::Error as IoError;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Output;
use std::process::Stdio;

use snafu::ResultExt;
use snafu::Snafu;

pub fn rebuild_with_streaming_output(
    target_img: &str,
    cache_img: &str,
    args: Vec<&str>,
)
    -> Result<ExitStatus, RebuildError<ExitStatus, SpawnDockerError>>
{
    rebuild_img(
        target_img,
        cache_img,
        args,
        |build_args| {
            let build_result = stream_docker(build_args)?;

            Ok((build_result, build_result.success()))
        },
    )
}

fn rebuild_img<F, V, E>(
    target_img: &str,
    cache_img: &str,
    args: Vec<&str>,
    run_docker: F,
)
    -> Result<V, RebuildError<V, E>>
where
    F: FnOnce(Vec<&str>) -> Result<(V, bool), E>,
    E: Error + 'static,
    V: Clone + Debug,
{
    // TODO Check the actual error, and return an error if `docker tag`
    // returned an unexpected error.
    let tag_result =
        Command::new("docker")
            .args(&["tag", target_img, cache_img])
            .output()
            .context(TagFailed)?;

    let tag_flag = &format!("--tag={}", target_img);

    // By default, Docker removes intermediate containers after a successful
    // build, but leaves them after a failed build. We use `--force-rm` to
    // remove them even if the build failed. See "Container Removal" in
    // `README.md` for more details.
    let mut build_args = vec!["build", tag_flag, "--force-rm"];

    build_args.extend(args);

    let (build_result, build_success) = run_docker(build_args)
        .context(BuildNewImageFailed)?;

    // We only attempt to remove or re-tag the cached image if the initial
    // tagging succeeded.
    if tag_result.status.success() {
        if build_success {
            assert_docker(&["rmi", cache_img])
                .with_context(|| RemoveOldImageFailed{
                    build_result: build_result.clone(),
                })?;
        } else {
            assert_docker(&["tag", cache_img, target_img])
                .with_context(|| UntagFailed{
                    build_result: build_result.clone(),
                })?;
        }
    }

    Ok(build_result)
}

#[allow(clippy::pub_enum_variant_names)]
#[derive(Debug, Snafu)]
pub enum RebuildError<T, E>
where
    E: Error + 'static
{
    TagFailed{source: IoError},
    BuildNewImageFailed{source: E},
    UntagFailed{source: AssertDockerError, build_result: T},
    RemoveOldImageFailed{source: AssertDockerError, build_result: T},
}

fn assert_docker<I, S>(args: I) -> Result<Output, AssertDockerError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output =
        Command::new("docker")
            .args(args)
            .output()
            .context(RunFailed)?;

    if !output.status.success() {
        return Err(AssertDockerError::NonZeroExit{output});
    }

    Ok(output)
}

#[derive(Debug, Snafu)]
pub enum AssertDockerError {
    RunFailed{source: IoError},
    NonZeroExit{output: Output},
}

// `stream_docker` runs a `docker` subcommand but passes the file descriptors
// for the standard streams of the current process to the child, so all input
// and output will be passed as if the subcommand was the current process.
pub fn stream_docker<I, S>(args: I) -> Result<ExitStatus, SpawnDockerError>
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
pub enum SpawnDockerError {
    SpawnFailed{source: IoError},
    WaitFailed{source: IoError},
}

pub fn rebuild_with_captured_output(
    target_img: &str,
    cache_img: &str,
    dockerfile: Option<File>,
    args: Vec<&str>,
)
    -> Result<Output, RebuildError<Output, RebuildWithCapturedOutputError>>
{
    rebuild_img(
        target_img,
        cache_img,
        args,
        |build_args| {
            let stdin_behaviour =
                if dockerfile.is_some() {
                    Stdio::piped()
                } else {
                    Stdio::null()
                };

            let mut docker_proc =
                Command::new("docker")
                    .args(build_args)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .stdin(stdin_behaviour)
                    .spawn()
                    .context(PipedSpawnFailed)?;

            if let Some(mut dockerfile) = dockerfile {
                // TODO `docker_proc.wait_with_output()` blocks if this block
                // doesn't surround the usage of `stdin`. This is likely due to
                // `stdin.take()` causing the child to be blocked on input,
                // which the new block explicitly drops, though this behaviour
                // should be confirmed and documented when time allows.

                let mut stdin = docker_proc.stdin.take()
                    .expect("`docker` process didn't contain a `stdin` pipe");

                io::copy(&mut dockerfile, &mut stdin)
                    .context(PipeDockerfileFailed)?;
            }

            let build_result = docker_proc.wait_with_output()
                .context(PipedWaitFailed)?;

            let success = build_result.status.success();

            Ok((build_result, success))
        },
    )
}

#[allow(clippy::pub_enum_variant_names)]
#[derive(Debug, Snafu)]
pub enum RebuildWithCapturedOutputError {
    PipedSpawnFailed{source: IoError},
    PipeDockerfileFailed{source: IoError},
    PipedWaitFailed{source: IoError},
}
