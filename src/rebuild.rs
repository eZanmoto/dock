// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::error::Error;
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

use docker;
use docker::AssertRunError;
use docker::GetImageIdsError;
use docker::StreamRunError;

pub fn rebuild_with_streaming_output(target_img: &str, args: Vec<&str>)
    -> Result<ExitStatus, RebuildError<ExitStatus, StreamRunError>>
{
    rebuild_img(
        target_img,
        args,
        |build_args| {
            let build_result = docker::stream_run(build_args)?;

            Ok((build_result, build_result.success()))
        },
    )
}

fn rebuild_img<F, V, E>(target_img: &str, args: Vec<&str>, run_docker: F)
    -> Result<V, RebuildError<V, E>>
where
    F: FnOnce(Vec<&str>) -> Result<(V, bool), E>,
    E: Error + 'static,
    V: Clone + Debug,
{
    let img_ids = docker::get_image_ids(target_img)
        .context(GetImageIdsBeforeBuildFailed)?;

    let maybe_old_img_id =
        match &img_ids[..] {
            [] => None,
            [id] => Some(id.clone()),
            _ => return Err(RebuildError::MultipleImageIdsBeforeBuild{
                ids: img_ids,
                repo: target_img.to_string(),
            }),
        };

    let tag_flag = &format!("--tag={}", target_img);

    // By default, Docker removes intermediate containers after a successful
    // build, but leaves them after a failed build. We use `--force-rm` to
    // remove them even if the build failed. See "Container Removal" in
    // `README.md` for more details.
    let mut build_args = vec!["build", tag_flag, "--force-rm"];

    build_args.extend(args);

    let (build_result, build_success) = run_docker(build_args)
        .context(BuildNewImageFailed)?;

    let img_ids = docker::get_image_ids(target_img)
        .context(GetImageIdsAfterBuildFailed)?;

    if !build_success {
        return Ok(build_result);
    }

    match &img_ids[..] {
        [new_img_id] => {
            if let Some(old_img_id) = maybe_old_img_id {
                if &old_img_id != new_img_id {
                    docker::assert_run(&["rmi", &old_img_id])
                        .with_context(|| RemoveOldImageFailed{
                            build_result: build_result.clone(),
                        })?;
                }
            }

            Ok(build_result)
        },
        [] => {
            let repo = target_img.to_string();

            Err(RebuildError::NoImageIdsAfterBuild{repo})
        },
        _ => {
            let repo = target_img.to_string();

            Err(RebuildError::MultipleImageIdsAfterBuild{ids: img_ids, repo})
        },
    }
}

#[allow(clippy::pub_enum_variant_names)]
#[derive(Debug, Snafu)]
pub enum RebuildError<T, E>
where
    E: Error + 'static
{
    #[snafu(display("Couldn't get Docker image IDs pre-build: {}", source))]
    GetImageIdsBeforeBuildFailed{source: GetImageIdsError},
    #[snafu(display("Couldn't get Docker image IDs post-build: {}", source))]
    GetImageIdsAfterBuildFailed{source: GetImageIdsError},
    #[snafu(display("Couldn't build a new Docker image: {}", source))]
    BuildNewImageFailed{source: E},
    #[snafu(display("Couldn't remove the old Docker image: {}", source))]
    RemoveOldImageFailed{source: AssertRunError, build_result: T},

    // NOTE The following are considered "developer errors" - they aren't
    // expected to happen, and if they do, then this may indicate that tighter
    // handling needs to be performed when retrieving image IDs.
    #[snafu(display(
        "(Dev Err) Multiple Docker images for '{}' were found pre-build: {:?}",
        repo,
        ids,
    ))]
    MultipleImageIdsBeforeBuild{ids: Vec<String>, repo: String},
    #[snafu(display(
        "(Dev Err) No Docker images for '{}' were found post-build",
        repo,
    ))]
    NoImageIdsAfterBuild{repo: String},
    #[snafu(display(
        "(Dev Err) Multiple Docker images for '{}' found post-build: {:?}",
        repo,
        ids,
    ))]
    MultipleImageIdsAfterBuild{ids: Vec<String>, repo: String},
}

pub fn rebuild_with_captured_output(
    target_img: &str,
    dockerfile: Option<File>,
    args: Vec<&str>,
)
    -> Result<Output, RebuildError<Output, RebuildWithCapturedOutputError>>
{
    rebuild_img(
        target_img,
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
    #[snafu(display("Couldn't spawn `docker` with piped output: {}", source))]
    PipedSpawnFailed{source: IoError},
    #[snafu(display("Couldn't pipe Dockerfile to `docker` STDIN: {}", source))]
    PipeDockerfileFailed{source: IoError},
    #[snafu(display("Couldn't wait for `docker`: {}", source))]
    PipedWaitFailed{source: IoError},
}
