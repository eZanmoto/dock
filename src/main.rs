// Copyright 2021-2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::collections::HashMap;
use std::env;
use std::fmt::Debug;
use std::fs::File;
use std::io;
use std::io::Error as IoError;
use std::io::Write;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::process;
use std::process::ExitStatus;
use std::process::Output;

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

mod docker;
mod fs;
mod rebuild;

use rebuild::RebuildError;
use rebuild::RebuildWithCapturedOutputError;

const CACHE_TAG_FLAG: &str = "cache-tag";
const TAGGED_IMG_FLAG: &str = "tagged-image";
const DOCKER_ARGS_FLAG: &str = "docker-args";
const ENV_FLAG: &str = "env";

fn main() {
    let rebuild_about: &str =
        &"Replace a tagged Docker image with a new build".to_string();

    let dock_file_name = "dock.yaml";
    let run_about: &str = &format!(
        "Run a command in an environment defined in `{}`",
        dock_file_name,
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
            let exit_code = run(dock_file_name, sub_args);
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

    let rebuild_result = rebuild::rebuild_with_streaming_output(
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

fn run(dock_file_name: &str, args: &ArgMatches) -> i32 {
    let cwd =
        match env::current_dir() {
            Ok(dir) => {
                dir
            },
            Err(err) => {
                eprintln!("couldn't get the current directory: {}", err);
                return 1;
            },
        };

    let (dock_dir, conf_reader) =
        match fs::find_and_open_file(&cwd, dock_file_name) {
            Ok(maybe_v) => {
                if let Some(v) = maybe_v {
                    v
                } else {
                    eprintln!("`{}` not found in path", dock_file_name);
                    return 1;
                }
            },
            Err(e) => {
                eprintln!("couldn't open `{}`: {}", dock_file_name, e);
                return 1;
            },
        };

    let conf: DockConfig =
        match serde_yaml::from_reader(conf_reader) {
            Ok(v) => {
                v
            },
            Err(e) => {
                eprintln!("couldn't parse `{}`: {}", dock_file_name, e);
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

    let target_img = new_tagged_img_name(&img_name, "latest");

    let ok = handle_rebuild_for_run(
        dock_dir,
        env_name,
        &env.context,
        &img_name,
        cache_tag,
        &target_img,
    );
    if !ok {
        return 1;
    }

    let extra_run_args =
        match args.values_of(DOCKER_ARGS_FLAG) {
            Some(vs) => vs.collect(),
            None => vec![],
        };

    let mut run_args = vec!["run", "--rm", &target_img];
    run_args.extend(extra_run_args);

    match docker::stream_run(run_args) {
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

fn handle_rebuild_for_run(
    dock_dir: PathBuf,
    env_name: &str,
    env_context: &Option<String>,
    img_name: &str,
    cache_tag: &str,
    target_img: &str,
) -> bool {
    let mut maybe_context_sub_path = None;
    if let Some(raw_context_sub_path) = env_context {
        let context_sub_path = Path::new(raw_context_sub_path);
        if path_contains_invalid_component(context_sub_path) {
            eprintln!(
                "context path must be relative, and can't contain traversal",
            );
            return false;
        }
        maybe_context_sub_path = Some(context_sub_path)
    }

    let cache_img = new_tagged_img_name(&img_name, cache_tag);

    let mut dockerfile_path = dock_dir.clone();
    dockerfile_path.push(format!("{}.Dockerfile", env_name));

    let docker_rebuild_input_result = new_docker_rebuild_input(
        dock_dir,
        &dockerfile_path.as_path(),
        maybe_context_sub_path,
    );
    let docker_rebuild_input =
        match docker_rebuild_input_result {
            Ok(v) => {
                v
            },
            Err(e) => {
                eprintln!(
                    "couldn't prepare parameters for docker rebuild: {}",
                    e,
                );
                return false;
            },
        };

    let rebuild_result = rebuild::rebuild_with_captured_output(
        target_img,
        &cache_img,
        docker_rebuild_input.dockerfile,
        docker_rebuild_input
            .args
            .iter()
            .map(AsRef::as_ref)
            .collect(),
    );

    handle_run_rebuild_result(rebuild_result)
}

fn new_docker_rebuild_input(
    dock_dir: PathBuf,
    dockerfile_path: &Path,
    maybe_context_sub_path: Option<&Path>,
)
    -> Result<DockerRebuildInput, NewDockerRebuildInputError>
{
    if let Some(context_sub_path) = maybe_context_sub_path {
        let mut context_path = dock_dir;
        context_path.push(context_sub_path);
        let raw_context_path =
            if let Some(v) = context_path.to_str() {
                v
            } else {
                return Err(
                    NewDockerRebuildInputError::InvalidUtf8InContextPath
                )
            };

        let raw_dockerfile_path =
            if let Some(v) = dockerfile_path.to_str() {
                v
            } else {
                return Err(
                    NewDockerRebuildInputError::InvalidUtf8InDockerfilePath
                )
            };

        Ok(DockerRebuildInput{
            dockerfile: None,
            args: vec![
                format!("--file={}", raw_dockerfile_path),
                raw_context_path.to_owned(),
            ]
        })
    } else {
        let dockerfile = File::open(&dockerfile_path)
            .context(OpenDockerfileFailed)?;

        Ok(DockerRebuildInput{
            dockerfile: Some(dockerfile),
            args: vec!["-".to_string()],
        })
    }
}

struct DockerRebuildInput {
    args: Vec<String>,
    dockerfile: Option<File>,
}

#[derive(Debug, Snafu)]
pub enum NewDockerRebuildInputError {
    InvalidUtf8InContextPath,
    InvalidUtf8InDockerfilePath,
    OpenDockerfileFailed{source: IoError},
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
