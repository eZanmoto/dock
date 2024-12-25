// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::error::Error;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fmt::Debug;
use std::fs::File;
use std::io::Error as IoError;
use std::path::PathBuf;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;

use snafu::ResultExt;
use snafu::Snafu;

use crate::canon_path::AbsPath;
use crate::docker;
use crate::docker::AssertRunError;
use crate::docker::StreamRunError;
use crate::logging_process;
use crate::logging_process::CommandLogger;
use crate::logging_process::RunError;

// TODO Take `args` as `&[&OsStr]`.
pub fn rebuild_with_streaming_output(
    target_img: &str,
    cache_img: &str,
    args: &[&str],
)
    -> Result<ExitStatus, RebuildError<ExitStatus, StreamRunError>>
{
    rebuild_img(
        target_img,
        cache_img,
        strs_to_os_strings(args),
        |build_args| {
            let build_result = docker::stream_run(build_args)?;

            Ok((build_result, build_result.success()))
        },
    )
}

fn strs_to_os_strings(strs: &[&str]) -> Vec<OsString> {
    strs
        .iter()
        .map(OsString::from)
        .collect()
}

fn rebuild_img<F, V, E>(
    target_img: &str,
    cache_img: &str,
    args: Vec<OsString>,
    build_img: F,
)
    -> Result<V, RebuildError<V, E>>
where
    F: FnOnce(Vec<OsString>) -> Result<(V, bool), E>,
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

    let tag_flag = &format!("--tag={target_img}");

    // By default, Docker removes intermediate containers after a successful
    // build, but leaves them after a failed build. We use `--force-rm` to
    // remove them even if the build failed. See "Container Removal" in
    // `README.md` for more details.
    let mut build_args: Vec<OsString> =
        strs_to_os_strings(&["build", tag_flag, "--force-rm"]);

    build_args.extend(args);

    let (build_result, build_success) = build_img(build_args)
        .context(BuildNewImageFailed)?;

    // We only attempt to remove or re-tag the cached image if the initial
    // tagging succeeded.
    if tag_result.status.success() {
        if build_success {
            docker::assert_run(&["rmi", cache_img])
                .with_context(|| RemoveOldImageFailed{
                    build_result: build_result.clone(),
                })?;
        } else {
            // TODO Investigate whether this is needed, and whether `cache_img`
            // still exists after this runs.
            docker::assert_run(&["tag", cache_img, target_img])
                .with_context(|| UntagFailed{
                    build_result: build_result.clone(),
                })?;
        }
    }
    Ok(build_result)
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
pub enum RebuildError<T, E>
where
    E: Error + 'static
{
    #[snafu(display("Couldn't tag Docker image: {}", source))]
    TagFailed{source: IoError},
    #[snafu(display("Couldn't build a new Docker image: {}", source))]
    BuildNewImageFailed{source: E},
    #[snafu(display("Couldn't remove the old Docker image: {}", source))]
    RemoveOldImageFailed{source: AssertRunError, build_result: T},
    #[snafu(display("Couldn't replace tag on Docker: {}", source))]
    UntagFailed{source: AssertRunError, build_result: T},
}

pub enum DockerContext {
    Empty{dockerfile: File},
    Dir{path: AbsPath, dockerfile: AbsPath},
}

pub fn rebuild(
    logger: &mut dyn CommandLogger,
    target_img: &str,
    cache_img: &str,
    context: DockerContext,
    extra_args: &[&str],
)
    -> Result<ExitStatus, RebuildError<ExitStatus, RunError>>
{
    let stdin;
    let mut args;
    match context {
        DockerContext::Empty{dockerfile} => {
            stdin = Stdio::from(dockerfile);
            args = vec![OsString::from("-")];
        },
        DockerContext::Dir{path, dockerfile} => {
            stdin = Stdio::null();

            let mut file_arg = OsString::from("--file=");
            file_arg.push(PathBuf::from(dockerfile).as_os_str());
            args = vec![file_arg, PathBuf::from(path).into_os_string()];
        },
    }

    args.extend(strs_to_os_strings(extra_args));

    rebuild_img(
        target_img,
        cache_img,
        args,
        |build_args| {
            let build_args: Vec<&OsStr> =
                build_args
                    .iter()
                    .map(OsStr::new)
                    .collect();

            let build_result = logging_process::run(
                logger,
                OsStr::new("docker"),
                &build_args,
                stdin,
            )?;

            let success = build_result.success();

            Ok((build_result, success))
        },
    )
}
