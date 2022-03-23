// Copyright 2021-2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::fs::File;
use std::io;
use std::io::Error as IoError;
use std::io::Write;
use std::path::Component;
use std::path::Path;
use std::process;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Output;
use std::process::Stdio;

extern crate clap;
extern crate serde;
extern crate snafu;

use clap::App;
use clap::AppSettings;
use clap::Arg;
use clap::ArgMatches;
use clap::SubCommand;
use serde::Deserialize;
use snafu::ResultExt;
use snafu::Snafu;

const CACHE_TAG_FLAG: &str = "cache-tag";
const TAGGED_IMG_FLAG: &str = "tagged-image";
const DOCKER_ARGS_FLAG: &str = "docker-args";
const ENV_FLAG: &str = "env";

fn main() {
    let rebuild_about: &str =
        &"Replace a tagged Docker image with a new build".to_string();

    let dock_config_path = "dock.yaml";
    let run_about: &str = &format!(
        "Run a command in an environment defined in `{}`",
        dock_config_path,
    );

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
                    .about(rebuild_about)
                    .args(&[
                        Arg::with_name(CACHE_TAG_FLAG)
                            .long(CACHE_TAG_FLAG)
                            .default_value("cached")
                            .help("The tag for the cache image")
                            .long_help(&format!(
                                "The tag to use for the image that will be \
                                 replaced by the rebuild. If an image with \
                                 the tagged name `{tagged_img_flag}` exists \
                                 then its tag will be replaced by \
                                 `{cache_tag_flag}` for the duration of the \
                                 rebuild.",
                                tagged_img_flag = TAGGED_IMG_FLAG,
                                cache_tag_flag = CACHE_TAG_FLAG,
                            )),
                        Arg::with_name(TAGGED_IMG_FLAG)
                            .required(true)
                            .help("The tagged name for the new image")
                            .long_help(
                                "The tagged name for the new image, in the \
                                 form `name:tag`.",
                            ),
                        Arg::with_name(DOCKER_ARGS_FLAG)
                            .multiple(true)
                            .help("Arguments to pass to `docker build`"),
                    ]),
                SubCommand::with_name("run")
                    .setting(AppSettings::TrailingVarArg)
                    .about(run_about)
                    .args(&[
                        Arg::with_name(CACHE_TAG_FLAG)
                            .long(CACHE_TAG_FLAG)
                            .default_value("cached")
                            .help("The tag for the cache image")
                            .long_help(&format!(
                                "The tag to use for the image that will be \
                                 replaced by the rebuild. If an image with \
                                 the tagged name `{tagged_img_flag}` exists \
                                 then its tag will be replaced by \
                                 `{cache_tag_flag}` for the duration of the \
                                 rebuild.",
                                tagged_img_flag = TAGGED_IMG_FLAG,
                                cache_tag_flag = CACHE_TAG_FLAG,
                            )),
                        Arg::with_name(ENV_FLAG)
                            .required(true)
                            .help("The environment to run"),
                        Arg::with_name(DOCKER_ARGS_FLAG)
                            .multiple(true)
                            .help("Arguments to pass to `docker build`"),
                    ]),
            ])
            .get_matches();

    match args.subcommand() {
        ("rebuild", Some(sub_args)) => {
            let docker_args =
                match sub_args.values_of(DOCKER_ARGS_FLAG) {
                    Some(vs) => vs.collect(),
                    None => vec![],
                };

            let exit_code = rebuild(
                sub_args.value_of(TAGGED_IMG_FLAG).unwrap(),
                sub_args.value_of(CACHE_TAG_FLAG).unwrap(),
                docker_args,
            );
            process::exit(exit_code);
        },
        ("run", Some(sub_args)) => {
            let exit_code = run(dock_config_path, sub_args);
            process::exit(exit_code);
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

fn rebuild(target_img: &str, cache_tag: &str, docker_args: Vec<&str>) -> i32 {
    let target_img_parts =
        target_img.split(':').collect::<Vec<&str>>();

    let img_name =
        if let [name, _tag] = target_img_parts.as_slice() {
            name
        } else {
            eprintln!(
                "`{}` must contain exactly one `:`",
                TAGGED_IMG_FLAG,
            );
            return 1;
        };

    let cache_img = new_tagged_img_name(img_name, cache_tag);

    if let Some(i) = index_of_first_unsupported_flag(&docker_args) {
        eprintln!("unsupported argument: `{}`", docker_args[i]);
        return 1;
    }

    let rebuild_result = rebuild_with_streaming_output(
        &target_img,
        &cache_img,
        docker_args,
    );
    match rebuild_result {
        Ok(exit_status) => {
            exit_code_from_exit_status(exit_status)
        },
        Err(e) => {
            eprintln!("{:?}", e);

            1
        },
    }
}

fn exit_code_from_exit_status(status: ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        code
    } else if status.success() {
        0
    } else {
        1
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

fn rebuild_with_streaming_output(
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

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
enum RebuildError<T, E>
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
enum AssertDockerError {
    RunFailed{source: IoError},
    NonZeroExit{output: Output},
}

// `stream_docker` runs a `docker` subcommand but passes the file descriptors
// for the standard streams of the current process to the child, so all input
// and output will be passed as if the subcommand was the current process.
fn stream_docker<I, S>(args: I) -> Result<ExitStatus, SpawnDockerError>
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
enum SpawnDockerError {
    SpawnFailed{source: IoError},
    WaitFailed{source: IoError},
}



#[derive(Debug, Deserialize)]
struct DockConfig {
    organisation: String,
    project: String,
    environments: HashMap<String, DockEnvironmentConfig>
}

#[derive(Debug, Deserialize)]
struct DockEnvironmentConfig {
    context: Option<String>
}

fn run(dock_config_path: &str, args: &ArgMatches) -> i32 {
    let conf_reader =
        match File::open(dock_config_path) {
            Ok(v) => {
                v
            },
            Err(e) => {
                eprintln!("couldn't open `{}`: {}", dock_config_path, e);
                return 1;
            },
        };

    let conf: DockConfig =
        match serde_yaml::from_reader(conf_reader) {
            Ok(v) => {
                v
            },
            Err(e) => {
                eprintln!("couldn't parse `{}`: {}", dock_config_path, e);
                return 1;
            },
        };

    let env_name = args.value_of(ENV_FLAG).unwrap();
    let cache_tag = args.value_of(CACHE_TAG_FLAG).unwrap();

    let env =
        if let Some(env) = conf.environments.get(env_name) {
            env
        } else {
            eprintln!("environment '{}' isn't defined", env_name);
            return 1;
        };

    let img_name = format!(
        "{}/{}.{}",
        conf.organisation,
        conf.project,
        env_name,
    );

    let cache_img = new_tagged_img_name(&img_name, cache_tag);

    let target_img = new_tagged_img_name(&img_name, "latest");

    let dfile_name = format!("{}.Dockerfile", env_name);
    let file_arg = format!("--file={}", dfile_name);
    let mut build_args = vec!["-"];
    let mut dockerfile = None;
    if let Some(c) = &env.context {
        // FIXME The context path should be defined relative to `dock.yaml`.

        if path_contains_invalid_component(Path::new(c)) {
            eprintln!("context path can't contain traversal (e.g. `..`)");
            return 1;
        }

        build_args = vec![&file_arg, &c];
    } else {
        dockerfile =
            match File::open(&dfile_name) {
                Ok(f) => {
                    Some(f)
                },
                Err(e) => {
                    eprintln!("couldn't open '{}': {:?}", dfile_name, e);
                    return 1;
                },
            };
    }

    let rebuild_result = rebuild_with_captured_output(
        &target_img,
        &cache_img,
        dockerfile,
        build_args,
    );
    if !handle_run_rebuild_result(rebuild_result) {
        return 1;
    }

    let extra_run_args =
        match args.values_of(DOCKER_ARGS_FLAG) {
            Some(vs) => vs.collect(),
            None => vec![],
        };

    let mut run_args = vec!["run", "--rm", &target_img];
    run_args.extend(extra_run_args);

    match stream_docker(run_args) {
        Ok(exit_status) => {
            exit_code_from_exit_status(exit_status)
        },
        Err(v) => {
            eprintln!("{:?}", v);

            1
        },
    }
}

fn new_tagged_img_name(img_name: &str, tag: &str) -> String {
    format!("{}:{}", img_name, tag)
}

fn rebuild_with_captured_output(
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

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
enum RebuildWithCapturedOutputError {
    PipedSpawnFailed{source: IoError},
    PipeDockerfileFailed{source: IoError},
    PipedWaitFailed{source: IoError},
}

// `handle_run_rebuild_result` returns `true` if `r` indicates a successful
// rebuild, and returns `false` otherwise.
fn handle_run_rebuild_result(
    r: Result<Output, RebuildError<Output, RebuildWithCapturedOutputError>>,
) -> bool {
    match r {
        Ok(Output{status, stdout, stderr}) => {
            // We ignore the status code returned "by the build step" because
            // there isn't anything to distinguish it from a status code
            // returned "by the run step".
            if status.success() {
                return true;
            }

            let result = io::stdout()
                .lock()
                .write_all(&stdout);
            if let Err(e) = result {
                eprintln!("couldn't write captured STDOUT: {}", e);
            }

            let result = io::stderr()
                .lock()
                .write_all(&stderr);
            if let Err(e) = result {
                eprintln!("couldn't write captured STDERR: {}", e);
            }
        },
        Err(v) => {
            eprintln!("{:?}", v);
        },
    }

    false
}

// TODO Consider returning the invalid component to support clearer error
// messages.
fn path_contains_invalid_component(p: &Path) -> bool {
    for c in p.components() {
        match c {
            Component::Normal(_) | Component::CurDir => {},
            _ => return true,
        }
    }

    false
}
