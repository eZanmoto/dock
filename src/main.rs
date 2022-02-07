// Copyright 2021-2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::env;
use std::ffi::OsStr;
use std::io::Error as IoError;
use std::process;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Output;

extern crate clap;
extern crate snafu;

use clap::App;
use clap::AppSettings;
use clap::Arg;
use clap::SubCommand;
use snafu::ResultExt;
use snafu::Snafu;

fn main() {
    let install_about: &str =
        &"Replace a tagged Docker image with a new build".to_string();
    let cache_tag_flag = "cache-tag";
    let tagged_img_flag = "tagged-image";
    let docker_args_flag = "docker-args";

    let args =
        App::new("dpnd")
            .version(env!("CARGO_PKG_VERSION"))
            .author(env!("CARGO_PKG_AUTHORS"))
            .about(env!("CARGO_PKG_DESCRIPTION"))
            .settings(&[
                AppSettings::SubcommandRequiredElseHelp,
                AppSettings::VersionlessSubcommands,
            ])
            .subcommands(vec![
                SubCommand::with_name("rebuild")
                    .setting(AppSettings::TrailingVarArg)
                    .about(install_about)
                    .args(&[
                        Arg::with_name(cache_tag_flag)
                            .long(cache_tag_flag)
                            .default_value("cached")
                            .help("The tag for the cache image")
                            .long_help(&format!(
                                "The tag to use for the image that will be \
                                 replaced by the rebuild. If an image with \
                                 the tagged name `{tagged_img_flag}` exists \
                                 then its tag will be replaced by \
                                 `{cache_tag_flag}` for the duration of the \
                                 rebuild.",
                                tagged_img_flag = tagged_img_flag,
                                cache_tag_flag = cache_tag_flag,
                            )),
                        Arg::with_name(tagged_img_flag)
                            .required(true)
                            .help("The tagged name for the new image")
                            .long_help(
                                "The tagged name for the new image, in the \
                                 form `name:tag`.",
                            ),
                        Arg::with_name(docker_args_flag)
                            .multiple(true)
                            .help("Arguments to pass to `docker build`"),
                    ]),
            ])
            .get_matches();

    match args.subcommand() {
        ("rebuild", Some(sub_args)) => {
            let target_img = sub_args.value_of(tagged_img_flag).unwrap();
            let cache_tag = sub_args.value_of(cache_tag_flag).unwrap();

            let target_img_parts =
                target_img.split(':').collect::<Vec<&str>>();

            let img_name =
                if let [name, _tag] = target_img_parts.as_slice() {
                    name
                } else {
                    eprintln!(
                        "`{}` must contain exactly one `:`",
                        tagged_img_flag,
                    );
                    process::exit(1);
                };

            let cache_img = format!("{}:{}", img_name, cache_tag);

            let docker_args =
                match sub_args.values_of(docker_args_flag) {
                    Some(vs) => vs.collect(),
                    None => vec![],
                };

            if let Some(i) = index_of_first_unsupported_flag(&docker_args) {
                eprintln!("unsupported argument: `{}`", docker_args[i]);
                process::exit(1);
            }

            match rebuild(&target_img, &cache_img, docker_args) {
                Ok(exit_status) => {
                    let exit_code =
                        if let Some(code) = exit_status.code() {
                            code
                        } else if exit_status.success() {
                            0
                        } else {
                            1
                        };

                    process::exit(exit_code);
                },
                Err(v) => eprintln!("{:?}", v),
            }
        },
        (arg_name, sub_args) => {
            // All subcommands defined in `args_defn` should be handled here,
            // so matching an unhandled command shouldn't happen.
            panic!(
                "unexpected command '{}' (arguments: '{:?}')",
                arg_name,
                sub_args,
            );
        },
    }
}

fn index_of_first_unsupported_flag(args: &[&str]) -> Option<usize> {
    // Note that this is a naive approach to checking whether the tag flag is
    // present, as it has the potential to give a false positive in the case
    // where the tag string is passed as a value to another flag. However, we
    // take this approach for simplicity, under the assumption that the case of
    // a tag string being passed as a value is unlikely. This functionality
    // would need to be refined if this assumption doesn't hold in practice.
    for (i, arg) in args.iter().enumerate() {
        let matched =
            arg == &"--force-rm"
            || arg == &"-t"
            || arg == &"--tag"
            || arg.starts_with("--tag=");

        if matched {
            return Some(i);
        }
    }

    None
}

fn rebuild(target_img: &str, cache_img: &str, args: Vec<&str>)
    -> Result<ExitStatus, RebuildError>
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
    // remove them even if the build failed.
    let mut build_args = vec!["build", tag_flag, "--force-rm"];

    build_args.extend(args);

    let build_status =
        stream_docker(build_args)
            .context(BuildNewImageFailed)?;

    // We only attempt to remove or re-tag the cached image if the initial
    // tagging succeeded.
    if tag_result.status.success() {
        if build_status.success() {
            assert_docker(&["rmi", cache_img])
                .context(RemoveOldImageFailed{build_status})?;
        } else {
            assert_docker(&["tag", cache_img, target_img])
                .context(UntagFailed{build_status})?;
        }
    }

    Ok(build_status)
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
enum RebuildError {
    TagFailed{source: IoError},
    BuildNewImageFailed{source: StreamDockerError},
    UntagFailed{source: AssertDockerError, build_status: ExitStatus},
    RemoveOldImageFailed{source: AssertDockerError, build_status: ExitStatus},
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
enum AssertDockerError {
    RunFailed{source: IoError},
    NonZeroExit{output: Output},
}

// `stream_docker` runs a `docker` subcommand but passes the file descriptors
// for the standard streams of the current process to the child, so all input
// and output will be passed as if the subcommand was the current process.
fn stream_docker<I, S>(args: I) -> Result<ExitStatus, StreamDockerError>
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
enum StreamDockerError {
    SpawnFailed{source: IoError},
    WaitFailed{source: IoError},
}
