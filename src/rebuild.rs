// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::error::Error;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fmt::Debug;
use std::fs::File;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::process::Stdio;

use snafu::ResultExt;
use snafu::Snafu;

use crate::canon_path::AbsPath;
use crate::docker;
use crate::docker::AssertRunError;
use crate::docker::GetImageIdsError;
use crate::docker::StreamRunError;
use crate::logging_process;
use crate::logging_process::CommandLogger;
use crate::logging_process::RunError;

// TODO Take `args` as `&[&OsStr]`.
pub fn rebuild_with_streaming_output(target_img: &str, args: &[&str])
    -> Result<ExitStatus, RebuildError<ExitStatus, StreamRunError>>
{
    rebuild_img(
        target_img,
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

fn rebuild_img<F, V, E>(target_img: &str, args: Vec<OsString>, build_img: F)
    -> Result<V, RebuildError<V, E>>
where
    F: FnOnce(Vec<OsString>) -> Result<(V, bool), E>,
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
    let mut build_args: Vec<OsString> =
        strs_to_os_strings(&["build", tag_flag, "--force-rm"]);

    build_args.extend(args);

    let (build_result, build_success) = build_img(build_args)
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

#[allow(clippy::enum_variant_names)]
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

pub enum DockerContext {
    Empty{dockerfile: File},
    Dir{path: AbsPath, dockerfile: AbsPath},
}

pub fn rebuild(
    logger: &mut dyn CommandLogger,
    target_img: &str,
    context: DockerContext,
)
    -> Result<ExitStatus, RebuildError<ExitStatus, RunError>>
{
    let stdin;
    let args;
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

    rebuild_img(
        target_img,
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
