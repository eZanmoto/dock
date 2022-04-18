// Copyright 2021-2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::collections::HashMap;
use std::env;
use std::env::VarError;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fmt::Debug;
use std::fs as std_fs;
use std::fs::File;
use std::io;
use std::io::Error as IoError;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::process;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Output;
use std::str;
use std::str::Utf8Error;

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
mod trie;

use rebuild::RebuildError;
use rebuild::RebuildWithCapturedOutputError;
use trie::InsertError;
use trie::Trie;

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

fn rebuild(target_img: &str, docker_args: Vec<&str>) -> i32 {
    if let Some(i) = index_of_first_unsupported_flag(&docker_args) {
        eprintln!("unsupported argument: `{}`", docker_args[i]);
        return 1;
    }

    let rebuild_result = rebuild::rebuild_with_streaming_output(
        &target_img,
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
    context: Option<String>,
    args: Option<Vec<String>>,
    mounts: Option<HashMap<String, String>>,
    enabled: Option<Vec<DockEnvironmentEnabledConfig>>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum DockEnvironmentEnabledConfig {
    LocalUserGroup,
    NestedDocker,
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

    let target_img = format!("{}:latest", &img_name);

    let ok = handle_rebuild_for_run(
        dock_dir.clone(),
        env_name,
        &env.context,
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

    let dock_dir =
        match abs_path_from_path_buf(&dock_dir) {
            Ok(v) => {
                v
            },
            Err(e) => {
                eprintln!("{:?}", e);

                return 1;
            },
        };

    let run_args =
        match prepare_run_args(env, target_img, &extra_run_args, &dock_dir) {
            Ok(v) => {
                v
            },
            Err(e) => {
                eprintln!("{:?}", e);

                return 1;
            },
        };

    match docker::stream_run(run_args) {
        Ok(exit_status) => {
            exit_code_from_exit_status(exit_status)
        },
        Err(e) => {
            eprintln!("{:?}", e);

            1
        },
    }
}

fn handle_rebuild_for_run(
    dock_dir: PathBuf,
    env_name: &str,
    env_context: &Option<String>,
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

fn prepare_run_args(
    env: &DockEnvironmentConfig,
    target_img: String,
    extra_args: &[&str],
    dock_dir: AbsPathRef,
)
    -> Result<Vec<String>, PrepareRunArgsError>
{
    let mut run_args = to_strings(&["run", "--rm"]);

    if let Some(args) = &env.args {
        run_args.extend(args.clone());
    }

    if let Some(enabled) = &env.enabled {
        if enabled.contains(&DockEnvironmentEnabledConfig::LocalUserGroup) {
            let user_id = run_command("id", &["--user"])
                .context(GetUserIdFailed)?;

            let group_id = run_command("id", &["--group"])
                .context(GetGroupIdFailed)?;

            let user_group =
                format!("{}:{}", user_id.trim_end(), group_id.trim_end());
            run_args.extend(to_strings(&["--user", &user_group]));
        }

        if enabled.contains(&DockEnvironmentEnabledConfig::NestedDocker) {
            let meta = std_fs::metadata(DOCKER_SOCK_PATH)
                .context(GetDockerSockMetadataFailed)?;

            let mount_spec = format!(
                "type=bind,src={docker_sock_path},dst={docker_sock_path}",
                docker_sock_path = DOCKER_SOCK_PATH,
            );
            run_args.extend(to_strings(&[
                &format!("--mount={}", mount_spec),
                &format!("--group-add={}", meta.gid()),
            ]));
        }
    }

    if let Some(mounts) = &env.mounts {
        let args = prepare_run_mount_args(dock_dir, mounts)
            .context(PrepareRunMountArgsFailed)?;

        run_args.extend(args);
    }

    run_args.push(target_img);

    run_args.extend(to_strings(&extra_args));

    Ok(run_args)
}

const DOCKER_SOCK_PATH: &str = "/var/run/docker.sock";

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
enum PrepareRunArgsError {
    GetUserIdFailed{source: RunCommandError},
    GetGroupIdFailed{source: RunCommandError},
    GetDockerSockMetadataFailed{source: IoError},
    PrepareRunMountArgsFailed{source: PrepareRunMountArgsError},
}

fn to_strings(strs: &[&str]) -> Vec<String> {
    strs
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<String>>()
}

fn run_command(prog: &str, args: &[&str]) -> Result<String, RunCommandError> {
    let output = assert_run(prog, args)
        .context(AssertRunFailed)?;

    let stdout_bytes = output.stdout;
    let stdout = str::from_utf8(&stdout_bytes)
        .with_context(|| ConvertStdoutToUtf8Failed{
            stdout_bytes: stdout_bytes.clone(),
        })?;

    Ok(stdout.to_string())
}

#[derive(Debug, Snafu)]
enum RunCommandError {
    AssertRunFailed{source: AssertRunError},
    ConvertStdoutToUtf8Failed{source: Utf8Error, stdout_bytes: Vec<u8>},
}

fn assert_run<I, S>(prog: &str, args: I) -> Result<Output, AssertRunError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output =
        Command::new(prog)
            .args(args)
            .output()
            .context(RunFailed)?;

    if !output.status.success() {
        return Err(AssertRunError::NonZeroExit{output});
    }

    Ok(output)
}

#[derive(Debug, Snafu)]
pub enum AssertRunError {
    RunFailed{source: IoError},
    NonZeroExit{output: Output},
}

fn prepare_run_mount_args(
    dock_dir: AbsPathRef,
    mounts: &HashMap<String, String>,
)
    -> Result<Vec<String>, PrepareRunMountArgsError>
{
    let cur_hostpaths = hostpaths()
        .context(GetHostpathsFailed)?;

    let mut new_hostpaths = vec![];
    for (raw_rel_outer_path, raw_inner_path) in mounts.iter() {
        let rel_outer_path = parse_rel_path(raw_rel_outer_path)
            .context(ParseConfigOuterPathFailed)?;

        let mut path = dock_dir.to_owned();
        abs_path_extend(&mut path, rel_outer_path);

        if let Some(hostpaths) = &cur_hostpaths {
            if let Some(p) = apply_hostpath(hostpaths, &path) {
                path = p;
            } else {
                return Err(PrepareRunMountArgsError::NoPathRouteOnHost{
                    // TODO Add `hostpaths` to the error context. This ideally
                    // requires `&Trie` to implement `Clone` so that a new,
                    // owned copy of `hostpaths` can be added to the error.
                    attempted_path: path,
                });
            }
        }

        let host_path =
            if let Some(s) = abs_path_display(&path) {
                s
            } else {
                return Err(PrepareRunMountArgsError::DisplayHostPathFailed);
            };

        new_hostpaths.push((host_path, raw_inner_path));
    }

    let mut args = vec![];

    for (host_path, inner_path) in &new_hostpaths {
        let mount_spec = format!(
            "type=bind,src={},dst={}",
            host_path,
            inner_path,
        );
        args.push(format!("--mount={}", mount_spec));
    }

    let rendered_hostpaths = new_hostpaths
        .into_iter()
        .map(|(hp, ip)| format!("{}:{}", hp, ip))
        .collect::<Vec<String>>()
        .join(":");

    args.push(format!(
        "--env={}={}",
        DOCK_HOSTPATHS_VAR_NAME,
        rendered_hostpaths,
    ));

    Ok(args)
}

#[derive(Debug, Snafu)]
enum PrepareRunMountArgsError {
    ParseConfigOuterPathFailed{source: NewRelPathError},
    GetHostpathsFailed{source: HostpathsError},
    NoPathRouteOnHost{attempted_path: AbsPath},
    DisplayHostPathFailed,
}

const DOCK_HOSTPATHS_VAR_NAME: &str = "DOCK_HOSTPATHS";

type Hostpaths = Trie<OsString, RelPath>;

fn hostpaths() -> Result<Option<Hostpaths>, HostpathsError> {
    let raw_hostpaths =
        match env::var(DOCK_HOSTPATHS_VAR_NAME) {
            Ok(v) => {
                v
            },
            Err(VarError::NotPresent) => {
                return Ok(None)
            },
            Err(VarError::NotUnicode(value)) => {
                return Err(HostpathsError::EnvVarIsNotUnicode{value});
            },
        };

    let raw_hostpaths: Vec<&str> = raw_hostpaths.split(':').collect();

    if raw_hostpaths.len() % 2 == 1 {
        return Err(HostpathsError::UnmatchedHostpath{
            hostpaths: to_strings(&raw_hostpaths),
        })
    }

    let mut hostpaths = Trie::new();
    for pair in raw_hostpaths.chunks(2) {
        if let [outer_path, inner_path] = pair {
            let outer_path = parse_abs_path(outer_path)
                .context(ParseOuterPathFailed)?;

            let inner_path = parse_abs_path(inner_path)
                .context(ParseInnerPathFailed)?;

            hostpaths.insert(&inner_path, outer_path)
                .context(AddHostpathFailed)?;
        } else {
            // `chunks(2)` should always return slices of length 2.
            panic!("chunk didn't have length 2: {:?}", pair);
        }
    }

    Ok(Some(hostpaths))
}

#[derive(Debug, Snafu)]
enum HostpathsError {
    EnvVarIsNotUnicode{value: OsString},
    UnmatchedHostpath{hostpaths: Vec<String>},
    ParseOuterPathFailed{source: NewAbsPathError},
    ParseInnerPathFailed{source: NewAbsPathError},
    AddHostpathFailed{source: InsertError},
}

fn apply_hostpath(hostpaths: &Hostpaths, path: AbsPathRef)
    -> Option<AbsPath>
{
    let (prefix, host_dir) = hostpaths.value_at_prefix(&path)?;

    let rel_path: Vec<OsString> =
        path
            .iter()
            .skip(prefix.len())
            .cloned()
            .collect();

    let mut host_path = host_dir.clone();
    host_path.extend(rel_path);

    Some(host_path)
}

type AbsPath = Vec<OsString>;

/// Returns the `AbsPath` parsed from `p`. `p` must begin with a "root
/// directory" component.
fn parse_abs_path(p: &str) -> Result<AbsPath, NewAbsPathError> {
    abs_path_from_path_buf(&Path::new(p).to_path_buf())
}

#[derive(Debug, Snafu)]
enum NewAbsPathError {
    EmptyAbsPath,
    // TODO We would ideally add the path component as a field on
    // `NoRootDirPrefix` and `SpecialComponentInAbsPath` to track the component
    // that was unexpected. However, the current version of `Snafu` being used
    // ["cannot use lifetime-parameterized errors as
    // sources"](https://github.com/shepmaster/snafu/issues/99), so we omit
    // this field for now.
    NoRootDirPrefix,
    SpecialComponentInAbsPath,
}

fn abs_path_from_path_buf(p: &PathBuf) -> Result<AbsPath, NewAbsPathError> {
    let mut components = p.components();

    if let Some(component) = components.next() {
        if component != Component::RootDir {
            return Err(NewAbsPathError::NoRootDirPrefix);
        }
    } else {
        return Err(NewAbsPathError::EmptyAbsPath);
    }

    let mut abs_path = vec![];
    for component in components {
        if let Component::Normal(c) = component {
            abs_path.push(c.to_os_string());
        } else {
            return Err(NewAbsPathError::SpecialComponentInAbsPath);
        }
    }

    Ok(abs_path)
}

// TODO `abs_path_display` should ideally return an error instead of `None` if
// there is a problem rendering a component of the path.
fn abs_path_display(abs_path: AbsPathRef) -> Option<String> {
    if abs_path.is_empty() {
        return Some(path::MAIN_SEPARATOR.to_string());
    }

    let mut string = String::new();
    for component in abs_path {
        if let Some(s) = component.to_str() {
            string += &path::MAIN_SEPARATOR.to_string();
            string += s;
        } else {
            return None;
        }
    }

    Some(string)
}

fn abs_path_extend(abs_path: &mut AbsPath, rel_path: RelPath) {
    abs_path.extend(rel_path)
}

type AbsPathRef<'a> = &'a [OsString];

type RelPath = Vec<OsString>;

/// Returns the `RelPath` parsed from `p`. `p` must begin with a "current
/// directory" component (i.e. `.`).
fn parse_rel_path(p: &str) -> Result<RelPath, NewRelPathError> {
    let mut components = Path::new(p).components();

    if let Some(component) = components.next() {
        if component != Component::CurDir {
            return Err(NewRelPathError::NoCurDirPrefix);
        }
    } else {
        return Err(NewRelPathError::EmptyRelPath);
    }

    let mut rel_path = vec![];
    for component in components {
        if let Component::Normal(c) = component {
            rel_path.push(c.to_os_string());
        } else {
            return Err(NewRelPathError::SpecialComponentInRelPath);
        }
    }

    Ok(rel_path)
}

#[derive(Debug, Snafu)]
enum NewRelPathError {
    EmptyRelPath,
    // TODO See `NewAbsPathError` for more details on adding `Component` fields
    // in error variants.
    NoCurDirPrefix,
    SpecialComponentInRelPath,
}
