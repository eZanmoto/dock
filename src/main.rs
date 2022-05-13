// Copyright 2021-2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::char;
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
extern crate serde_yaml;
extern crate snafu;

use clap::App;
use clap::AppSettings;
use clap::Arg;
use clap::ArgMatches;
use clap::SubCommand;
use serde::Deserialize;
use serde_yaml::Error as SerdeYamlError;
use serde_yaml::Value;
use snafu::ResultExt;
use snafu::OptionExt;
use snafu::Snafu;

mod docker;
mod fs;
mod rebuild;
mod trie;

use docker::AssertRunError as DockerAssertRunError;
use docker::StreamRunError;
use fs::FindAndOpenFileError;
use rebuild::RebuildError;
use rebuild::RebuildWithCapturedOutputError;
use trie::InsertError;
use trie::Trie;

const TAGGED_IMG_FLAG: &str = "tagged-image";
const DOCKER_ARGS_FLAG: &str = "docker-args";
const ENV_FLAG: &str = "env";

fn main() {
    let rebuild_about: &str =
        "Replace a tagged Docker image with a new build";

    let dock_file_name = "dock.yaml";
    let run_about: &str = &format!(
        "Run a command in an environment defined in `{}`",
        dock_file_name,
    );
    let shell_about: &str = &format!(
        "Start a shell in an environment defined in `{}`",
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
                SubCommand::with_name("shell")
                    .about(shell_about)
                    .args(&[
                        Arg::with_name(ENV_FLAG)
                            .help("The environment to run"),
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
        ("shell", Some(sub_args)) => {
            let exit_code = shell(dock_file_name, sub_args);
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
        target_img,
        docker_args,
    );
    match rebuild_result {
        Ok(exit_status) => {
            exit_code_from_exit_status(exit_status)
        },
        Err(e) => {
            eprintln!("{}", e);

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

#[derive(Deserialize)]
struct DockConfig {
    schema_version: String,
    organisation: String,
    project: String,
    environments: HashMap<String, DockEnvironmentConfig>
}

// TODO Consider whether to automatically deserialise `PathBuf`s using `serde`,
// or to read them as `String`s and parse them directly.
#[derive(Debug, Deserialize)]
struct DockEnvironmentConfig {
    context: Option<PathBuf>,
    workdir: Option<String>,
    args: Option<Vec<String>>,
    env: Option<HashMap<String, String>>,
    cache_volumes: Option<HashMap<String, PathBuf>>,
    mounts: Option<HashMap<PathBuf, PathBuf>>,
    mount_local: Option<Vec<DockEnvironmentMountLocalConfig>>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum DockEnvironmentMountLocalConfig {
    User,
    Group,
    ProjectDir,
    Docker,
}

fn run(dock_file_name: &str, args: &ArgMatches) -> i32 {
    handle_run_with_extra_prefix_args(dock_file_name, args, vec![])
}

fn handle_run_with_extra_prefix_args(
    dock_file_name: &str,
    args: &ArgMatches,
    extra_prefix_args: Vec<String>,
) -> i32 {
    // TODO Make this variable required.
    let env_name = args.value_of(ENV_FLAG).unwrap();

    let extra_run_args =
        match args.values_of(DOCKER_ARGS_FLAG) {
            Some(vs) => vs.collect(),
            None => vec![],
        };

    let result = run_with_extra_prefix_args(
        dock_file_name,
        env_name,
        extra_prefix_args,
        &extra_run_args,
    );
    match result {
        Ok(exit_status) => {
            exit_code_from_exit_status(exit_status)
        },
        Err(e) => {
            match e {
                RunWithExtraPrefixArgsError::RebuildForRunFailed{
                    source: RebuildForRunError::RebuildFailed{
                        stdout,
                        stderr,
                        ..
                    },
                } => {
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
                _ => {
                    eprintln!("{}", e);
                },
            }

            1
        },
    }
}

fn run_with_extra_prefix_args(
    dock_file_name: &str,
    env_name: &str,
    extra_prefix_args: Vec<String>,
    extra_run_args: &[&str],
) -> Result<ExitStatus, RunWithExtraPrefixArgsError> {
    let cwd = env::current_dir()
        .context(GetCurrentDirFailed)?;

    let (dock_dir, conf_reader) = fs::find_and_open_file(&cwd, dock_file_name)
        .context(OpenDockFileFailed{
            dock_file_name: dock_file_name.to_string(),
        })?
        .context(DockFileNotFound{
            dock_file_name: dock_file_name.to_string(),
        })?;

    let conf = parse_dock_config(conf_reader)
        .context(ParseDockConfigFailed{
            dock_file_name: dock_file_name.to_string(),
        })?;

    let env = conf.environments.get(env_name)
        .context(EnvironmentNotFound{env_name})?;

    let img_name = format!(
        "{}/{}.{}",
        conf.organisation,
        conf.project,
        env_name,
    );

    let target_img = format!("{}:latest", &img_name);

    let env_context =
        if let Some(path) = &env.context {
            let p = rel_path_from_path_buf(path)
                .context(RelPathFromContextPathFailed)?;

            Some(p)
        } else {
            None
        };

    rebuild_for_run(
        dock_dir.clone(),
        env_name,
        &env_context,
        &target_img,
    )
        .context(RebuildForRunFailed)?;

    let proj = Project{org: conf.organisation, name: conf.project};

    let exit_status = run_for_run(
        &dock_dir,
        &proj,
        env_name,
        env,
        target_img,
        extra_prefix_args,
        extra_run_args,
    )
        .context(RunForRunFailed)?;

    Ok(exit_status)
}

// TODO The following variants don't need to contain `dock_file_name` as a
// field because it's passed to the `run_with_extra_prefix_args`, but we
// include it for now for simplicity.
#[derive(Debug, Snafu)]
enum RunWithExtraPrefixArgsError {
    #[snafu(display("Couldn't get the current directory: {}", source))]
    GetCurrentDirFailed{source: IoError},
    #[snafu(display("Couldn't find '{}'", dock_file_name))]
    DockFileNotFound{dock_file_name: String},
    #[snafu(display("Couldn't open '{}': {}", dock_file_name, source))]
    OpenDockFileFailed{source: FindAndOpenFileError, dock_file_name: String},
    #[snafu(display("Couldn't parse '{}': {}", dock_file_name, source))]
    ParseDockConfigFailed{
        source: ParseDockConfigError,
        dock_file_name: String,
    },
    #[snafu(display("Dock environment '{}' isn't defined", env_name))]
    EnvironmentNotFound{env_name: String},
    #[snafu(display(
        "Couldn't get path to the context directory as a relative path: {}",
        source,
    ))]
    RelPathFromContextPathFailed{source: NewRelPathError},
    #[snafu(display("{}", source))]
    RebuildForRunFailed{source: RebuildForRunError},
    #[snafu(display("{}", source))]
    RunForRunFailed{source: RunForRunError},
}

fn parse_dock_config(file: File) -> Result<DockConfig, ParseDockConfigError> {
    let conf_value: Value = serde_yaml::from_reader(file)
        .context(ParseYamlFailed)?;

    if let Some(vsn) = conf_value.get("schema_version") {
        if vsn != "0.1" {
            // TODO Add `vsn` to the error context.
            return Err(ParseDockConfigError::UnsupportedSchemaVersion);
        }
    } else {
        return Err(ParseDockConfigError::MissingSchemaVersion);
    }

    let conf: DockConfig = serde_yaml::from_value(conf_value)
        .context(ParseSchemaFailed)?;

    // `schema_version` isn't used after the configuration has been
    // deserialised, but we assign it to an unused variable to prevent Clippy
    // from alerting us about the unused field.
    #[allow(clippy::no_effect_underscore_binding)]
    let _vsn = &conf.schema_version;

    Ok(conf)
}

#[derive(Debug, Snafu)]
pub enum ParseDockConfigError {
    #[snafu(display("Couldn't parse: {}", source))]
    ParseYamlFailed{source: SerdeYamlError},
    #[snafu(display("Only `schema_version` 0.1 is currently supported"))]
    UnsupportedSchemaVersion,
    #[snafu(display("Missing `schema_version` field"))]
    MissingSchemaVersion,
    #[snafu(display("Parsed YAML didn't conform to schema: {}", source))]
    ParseSchemaFailed{source: SerdeYamlError},
}

#[derive(Debug)]
struct Project{
    org: String,
    name: String,
}

fn rebuild_for_run(
    dock_dir: PathBuf,
    env_name: &str,
    maybe_context_sub_path: &Option<RelPath>,
    img: &str,
)
    -> Result<(), RebuildForRunError>
{
    let mut dockerfile_path = dock_dir.clone();
    dockerfile_path.push(format!("{}.Dockerfile", env_name));

    let docker_rebuild_input_result = new_docker_rebuild_input(
        dock_dir,
        dockerfile_path.as_path(),
        maybe_context_sub_path,
    );
    let docker_rebuild_input = docker_rebuild_input_result
        .context(NewDockerRebuildInputFailed)?;

    let output = rebuild::rebuild_with_captured_output(
        img,
        docker_rebuild_input.dockerfile,
        docker_rebuild_input
            .args
            .iter()
            .map(AsRef::as_ref)
            .collect(),
    )
        .context(RebuildWithCapturedOutputFailed{img: img.to_string()})?;

    let Output{status, stdout, stderr} = output;

    if !status.success() {
        return Err(RebuildForRunError::RebuildFailed{
            stdout,
            stderr,
            img: img.to_string(),
        });
    }

    // We ignore the status code returned "by the build step" because there
    // isn't anything to distinguish it from a status code returned "by the run
    // step".
    Ok(())
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
enum RebuildForRunError {
    #[snafu(display("Couldn't prepare input for `dock rebuild`: {}", source))]
    NewDockerRebuildInputFailed{source: NewDockerRebuildInputError},
    #[snafu(display("Couldn't rebuild '{}': {}", img, source))]
    RebuildWithCapturedOutputFailed{
        source: RebuildError<Output, RebuildWithCapturedOutputError>,
        img: String,
    },
    #[snafu(display("Couldn't rebuild '{}'", img))]
    RebuildFailed{img: String, stdout: Vec<u8>, stderr: Vec<u8>},
}

fn new_docker_rebuild_input(
    dock_dir: PathBuf,
    dockerfile_path: &Path,
    maybe_context_sub_path: &Option<RelPath>,
)
    -> Result<DockerRebuildInput, NewDockerRebuildInputError>
{
    if let Some(context_sub_path) = maybe_context_sub_path {
        let mut context_path = dock_dir;
        path_buf_extend(&mut context_path, context_sub_path);

        let context_path_cli_arg = context_path.to_str()
            .context(InvalidUtf8InContextPath{path: context_path.clone()})?;

        let dockerfile_path_cli_arg = dockerfile_path.to_str()
            .context(InvalidUtf8InDockerfilePath{
                path: dockerfile_path.to_path_buf(),
            })?;

        Ok(DockerRebuildInput{
            dockerfile: None,
            args: vec![
                format!("--file={}", dockerfile_path_cli_arg),
                context_path_cli_arg.to_owned(),
            ]
        })
    } else {
        let dockerfile = File::open(&dockerfile_path)
            .context(OpenDockerfileFailed{path: dockerfile_path})?;

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
    #[snafu(display(
        "The path to the Docker context ('{}') contained invalid UTF-8",
        path.display(),
    ))]
    InvalidUtf8InContextPath{path: PathBuf},
    #[snafu(display(
        "The path to the Dockerfile ('{}') contained invalid UTF-8",
        path.display(),
    ))]
    InvalidUtf8InDockerfilePath{path: PathBuf},
    #[snafu(display(
        "Couldn't open the Dockerfile '{}': {}",
        path.display(),
        source,
    ))]
    OpenDockerfileFailed{source: IoError, path: PathBuf},
}

fn run_for_run(
    dock_dir: &Path,
    proj: &Project,
    env_name: &str,
    env: &DockEnvironmentConfig,
    target_img: String,
    extra_prefix_args: Vec<String>,
    extra_run_args: &[&str],
)
    -> Result<ExitStatus, RunForRunError>
{
    let dock_dir = abs_path_from_path_buf(dock_dir)
        .context(AbsPathFromDockDirFailed)?;

    let run_args = prepare_run_args(
        proj,
        env_name,
        env,
        target_img,
        extra_run_args,
        &dock_dir,
        extra_prefix_args,
    )
        .context(PrepareRunArgsFailed)?;

    let exit_status = docker::stream_run(run_args)
        .context(DockerRunFailed)?;

    Ok(exit_status)
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
enum RunForRunError {
    #[snafu(display(
        "Couldn't get path to current Dock directory as an absolute path: {}",
        source,
    ))]
    AbsPathFromDockDirFailed{source: NewAbsPathError},
    #[snafu(display(
        "Couldn't prepare arguments for `docker run`: {}",
        source,
    ))]
    PrepareRunArgsFailed{source: PrepareRunArgsError},
    #[snafu(display("`docker run` failed: {}", source))]
    DockerRunFailed{source: StreamRunError},
}

fn prepare_run_args(
    proj: &Project,
    env_name: &str,
    env: &DockEnvironmentConfig,
    target_img: String,
    extra_args: &[&str],
    dock_dir: AbsPathRef,
    extra_prefix_args: Vec<String>,
)
    -> Result<Vec<String>, PrepareRunArgsError>
{
    let mut run_args = to_strings(&["run", "--rm"]);

    run_args.extend(extra_prefix_args);

    if let Some(cache_volumes) = &env.cache_volumes {
        let args = prepare_run_cache_volumes_args(
            cache_volumes,
            proj,
            env_name,
            &target_img,
        )
            .context(PrepareRunCacheVolumesArgsFailed)?;

        run_args.extend(args);
    }

    if let Some(args) = &env.args {
        run_args.extend(args.clone());
    }

    if let Some(dir) = &env.workdir {
        run_args.push(format!("--workdir={}", dir));
    }

    if let Some(env_vars) = &env.env {
        for (k, v) in env_vars {
            run_args.push(format!("--env={}={}", k, v));
        }
    }

    let cur_hostpaths = hostpaths()
        .context(GetHostpathsFailed)?;

    if let Some(mount_local) = &env.mount_local {
        let args = prepare_run_mount_local_args(
            mount_local,
            dock_dir,
            &cur_hostpaths,
            &env.workdir,
        )
            .context(PrepareRunMountLocalArgsFailed)?;

        run_args.extend(args);
    }

    if let Some(mounts) = &env.mounts {
        let mut parsed_mounts = vec![];
        for (rel_outer_path, inner_path) in mounts.iter() {
            let rel_outer_path = rel_path_from_path_buf(rel_outer_path)
                .context(ParseConfigOuterPathFailed{
                    rel_outer_path,
                    inner_path,
                })?;

            parsed_mounts.push((rel_outer_path, inner_path));
        }

        let args =
            prepare_run_mount_args(dock_dir, &parsed_mounts, &cur_hostpaths)
                .context(PrepareRunMountArgsFailed)?;

        run_args.extend(args);
    }

    run_args.push(target_img);

    run_args.extend(to_strings(extra_args));

    Ok(run_args)
}

const DOCKER_SOCK_PATH: &str = "/var/run/docker.sock";

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
enum PrepareRunArgsError {
    #[snafu(display(
        "Couldn't prepare cache volume arguments for `docker run`: {}",
        source,
    ))]
    PrepareRunCacheVolumesArgsFailed{source: PrepareRunCacheVolumesArgsError},
    #[snafu(display(
        "Couldn't prepare \"local mount\" arguments for `docker run`: {}",
        source,
    ))]
    PrepareRunMountLocalArgsFailed{source: PrepareRunMountLocalArgsError},
    #[snafu(display(
        "Couldn't parse `mount` configuration for '{}' -> '{}' mapping: {}",
        source,
        rel_outer_path.display(),
        inner_path.display(),
    ))]
    ParseConfigOuterPathFailed{
        source: NewRelPathError,
        rel_outer_path: PathBuf,
        inner_path: PathBuf,
    },
    #[snafu(display("Couldn't get hostpaths: {}", source))]
    GetHostpathsFailed{source: HostpathsError},
    #[snafu(display(
        "Couldn't prepare \"mount\" arguments for `docker run`: {}",
        source,
    ))]
    PrepareRunMountArgsFailed{source: PrepareRunMountArgsError},
}

// TODO This method doesn't just prepare the cache volume arguments for the
// `docker run` command, but also creates the volumes (if they don't exist) and
// changes their permissions. This responsibility should ideally be moved to a
// dedicated function of its own.
fn prepare_run_cache_volumes_args(
    cache_volumes: &HashMap<String, PathBuf>,
    proj: &Project,
    env_name: &str,
    target_img: &str,
)
    -> Result<Vec<String>, PrepareRunCacheVolumesArgsError>
{
    let mut args = vec![];

    for (name, path) in cache_volumes.iter() {
        let path_abs_path = abs_path_from_path_buf(path)
            .context(CacheVolDirAsAbsPathFailed)?;

        let path_cli_arg = abs_path_display(&path_abs_path)
            .context(RenderCacheVolDirFailed{dir: path_abs_path})?;

        let vol_name = format!(
            "{}.{}.{}.cache.{}",
            proj.org,
            proj.name,
            env_name,
            name,
        );
        let mount_spec =
            format!("type=volume,src={},dst={}", vol_name, path_cli_arg);
        let mount_arg = format!("--mount={}", mount_spec);

        docker::assert_run(&[
            "run",
            "--rm",
            "--user=root",
            &mount_arg,
            target_img,
            "chmod",
            // We would ideally use `--recursive` instead of `-R` in order
            // to be more explicit, but in practice, `-R` has been found to
            // be available in more `chmod` implementations (notably, the
            // implementation used in `busybox`/`alpine` doesn't support
            // `--recursive`).
            "-R",
            "0777",
            &path_cli_arg,
        ])
            .context(ChangeCacheOwnershipFailed)?;

        args.push(mount_arg);
    }

    Ok(args)
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
enum PrepareRunCacheVolumesArgsError {
    #[snafu(display(
        "Couldn't convert the cache volume directory to an absolute path: {}",
        source,
    ))]
    CacheVolDirAsAbsPathFailed{source: NewAbsPathError},
    #[snafu(display(
        "Couldn't render the cache volume directory (lossy rendering: '{}')",
        abs_path_display_lossy(dir),
    ))]
    RenderCacheVolDirFailed{dir: AbsPath},
    #[snafu(display("Couldn't set the ownership of the cache: {}", source))]
    ChangeCacheOwnershipFailed{source: DockerAssertRunError},
}

fn prepare_run_mount_local_args(
    mount_local: &[DockEnvironmentMountLocalConfig],
    dock_dir: AbsPathRef,
    cur_hostpaths: &Option<Hostpaths>,
    workdir: &Option<String>,
)
    -> Result<Vec<String>, PrepareRunMountLocalArgsError>
{
    let mut args = vec![];

    if mount_local.contains(&DockEnvironmentMountLocalConfig::User) {
        let user_id = run_command("id", &["--user"])
            .context(GetUserIdFailed)?;

        if mount_local.contains(&DockEnvironmentMountLocalConfig::Group) {
            let group_id = run_command("id", &["--group"])
                .context(GetGroupIdFailed)?;

            let user_group =
                format!("{}:{}", user_id.trim_end(), group_id.trim_end());
            args.extend(to_strings(&["--user", &user_group]));
        } else {
            args.extend(to_strings(&["--user", user_id.trim_end()]));
        }
    } else if mount_local.contains(&DockEnvironmentMountLocalConfig::Group) {
        return Err(PrepareRunMountLocalArgsError::GroupMountedWithoutUser);
    }

    if mount_local.contains(&DockEnvironmentMountLocalConfig::Docker) {
        let meta = std_fs::metadata(DOCKER_SOCK_PATH)
            .context(GetDockerSockMetadataFailed)?;

        let mount_spec = format!(
            "type=bind,src={docker_sock_path},dst={docker_sock_path}",
            docker_sock_path = DOCKER_SOCK_PATH,
        );
        args.extend(to_strings(&[
            &format!("--mount={}", mount_spec),
            &format!("--group-add={}", meta.gid()),
        ]));
    }

    if mount_local.contains(&DockEnvironmentMountLocalConfig::ProjectDir) {
        // TODO Add `cur_hostpaths` to the error context. See the comment
        // above `NoPathRouteOnHost` for more details.
        let proj_dir_host_path = apply_hostpath(cur_hostpaths, dock_dir)
            .context(NoProjectPathRouteOnHost{attempted_path: dock_dir})?;

        let proj_dir_cli_arg = abs_path_display(&proj_dir_host_path)
            .context(RenderProjectDirFailed{dir: proj_dir_host_path})?;

        let workdir = workdir.as_ref()
            .context(WorkdirNotSet)?;

        let mount_spec =
            format!( "type=bind,src={},dst={}", proj_dir_cli_arg, workdir);
        args.push(format!("--mount={}", mount_spec));
    }

    Ok(args)
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
enum PrepareRunMountLocalArgsError {
    #[snafu(display("Couldn't get user ID for the active user: {}", source))]
    GetUserIdFailed{source: RunCommandError},
    #[snafu(display("Couldn't get group ID for the active user: {}", source))]
    GetGroupIdFailed{source: RunCommandError},
    #[snafu(display("local `group` was mounted without `user`"))]
    GroupMountedWithoutUser,
    #[snafu(display("Couldn't get metadata for Docker socket: {}", source))]
    GetDockerSockMetadataFailed{source: IoError},
    #[snafu(display(
        "No route to the project path '{}' was found on the host",
        abs_path_display_lossy(attempted_path),
    ))]
    NoProjectPathRouteOnHost{attempted_path: AbsPath},
    #[snafu(display(
        "Couldn't render the project directory (lossy rendering: '{}')",
        abs_path_display_lossy(dir),
    ))]
    RenderProjectDirFailed{dir: AbsPath},
    #[snafu(display("`workdir` is required when `project_dir` is mounted"))]
    WorkdirNotSet,
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
    #[snafu(display("Couldn't run the command: {}", source))]
    AssertRunFailed{source: AssertRunError},
    #[snafu(display("Couldn't convert STDOUT to UTF-8: {}", source))]
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
    #[snafu(display("Couldn't run the command: {}", source))]
    RunFailed{source: IoError},
    #[snafu(display("Command exited with a non-zero status: {:?}", output))]
    NonZeroExit{output: Output},
}

fn prepare_run_mount_args(
    dock_dir: AbsPathRef,
    mounts: &[(RelPath, &PathBuf)],
    cur_hostpaths: &Option<Hostpaths>,
)
    -> Result<Vec<String>, PrepareRunMountArgsError>
{
    let mut hostpath_cli_args = vec![];
    for (rel_outer_path, inner_path) in mounts {
        let mut path = dock_dir.to_owned();
        abs_path_extend(&mut path, rel_outer_path.clone());

        // TODO Add `cur_hostpaths` to the error context. This ideally requires
        // `&Trie` to implement `Clone` so that a new, owned copy of
        // `cur_hostpaths` can be added to the error.
        path = apply_hostpath(cur_hostpaths, &path)
            .context(NoPathRouteOnHost{attempted_path: path})?;

        let host_path_cli_arg = abs_path_display(&path)
            .context(RenderHostPathFailed{
                path,
                inner_path: (*inner_path).clone(),
            })?;

        let inner_path_os_string = (*inner_path).clone().into_os_string();
        let inner_path_cli_arg =
            match inner_path_os_string.into_string() {
                Ok(arg) => {
                    arg
                },
                Err(path) => {
                    let e = PrepareRunMountArgsError::InnerPathAsCliArgFailed{
                        path,
                    };

                    return Err(e);
                },
            };

        hostpath_cli_args.push((host_path_cli_arg, inner_path_cli_arg));
    }

    let mut args = vec![];

    for (host_path, inner_path) in &hostpath_cli_args {
        let mount_spec = format!(
            "type=bind,src={},dst={}",
            host_path,
            inner_path,
        );
        args.push(format!("--mount={}", mount_spec));
    }

    let rendered_hostpaths = hostpath_cli_args
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
    #[snafu(display(
        "No route to the path '{}' was found on the host",
        abs_path_display_lossy(attempted_path),
    ))]
    NoPathRouteOnHost{
        attempted_path: AbsPath,
    },
    #[snafu(display(
        "Couldn't render the hostpath mapping to '{}' (lossy rendering: '{}')",
        inner_path.display(),
        abs_path_display_lossy(path),
    ))]
    RenderHostPathFailed{
        path: AbsPath,
        inner_path: PathBuf,
    },
    #[snafu(display(
        "Couldn't render the inner path '{}' as a CLI argument",
        PathBuf::from(path).display(),
    ))]
    InnerPathAsCliArgFailed{
        path: OsString,
    },
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
            let abs_outer_path = parse_abs_path(outer_path)
                .context(ParseOuterPathFailed{
                    outer_path: (*outer_path).to_string(),
                    inner_path: (*inner_path).to_string(),
                })?;

            let abs_inner_path = parse_abs_path(inner_path)
                .context(ParseInnerPathFailed{
                    outer_path: (*outer_path).to_string(),
                    inner_path: (*inner_path).to_string(),
                })?;

            match hostpaths.insert(&abs_inner_path, abs_outer_path) {
                Ok(()) => {},
                Err(err) => {
                    let e =
                        match err {
                            InsertError::EmptyKey =>
                                HostpathsError::EmptyInnerPath{
                                    outer_path: (*outer_path).to_string(),
                                },
                            InsertError::PrefixContainsValue =>
                                HostpathsError::InnerPathAncestorHasMapping{
                                    outer_path: (*outer_path).to_string(),
                                    inner_path: (*inner_path).to_string(),
                                },
                            InsertError::DirAtKey =>
                                HostpathsError::InnerPathDescendentHasMapping{
                                    outer_path: (*outer_path).to_string(),
                                    inner_path: (*inner_path).to_string(),
                                },
                        };

                    return Err(e);
                },
            }
        } else {
            // `chunks(2)` should always return slices of length 2.
            panic!("chunk didn't have length 2: {:?}", pair);
        }
    }

    Ok(Some(hostpaths))
}

#[derive(Debug, Snafu)]
enum HostpathsError {
    #[snafu(display(
        "The value of '${}' isn't unicode",
        DOCK_HOSTPATHS_VAR_NAME,
    ))]
    EnvVarIsNotUnicode{value: OsString},
    #[snafu(display(
        "'${}' has an unmatched hostpath",
        DOCK_HOSTPATHS_VAR_NAME,
    ))]
    UnmatchedHostpath{hostpaths: Vec<String>},
    #[snafu(display(
        "Couldn't parse '{}' as an absolute path (mapped to '{}'): {}",
        inner_path,
        outer_path,
        source,
    ))]
    ParseOuterPathFailed{
        source: NewAbsPathError,
        outer_path: String,
        inner_path: String,
    },
    #[snafu(display(
        "Couldn't parse '{}' as an absolute path (mapped to '{}'): {}",
        inner_path,
        outer_path,
        source,
    ))]
    ParseInnerPathFailed{
        source: NewAbsPathError,
        outer_path: String,
        inner_path: String,
    },
    #[snafu(display("The path '{}' maps to an empty path", outer_path))]
    EmptyInnerPath{outer_path: String},
    #[snafu(display(
        "A hostname maps to an ancestor of '{}' (which is mapped-to by '{}')",
        inner_path,
        outer_path,
    ))]
    InnerPathAncestorHasMapping{outer_path: String, inner_path: String},
    #[snafu(display(
        "A hostname maps to a descendant of '{}' (which is mapped-to by '{}')",
        inner_path,
        outer_path,
    ))]
    InnerPathDescendentHasMapping{outer_path: String, inner_path: String},
}

fn apply_hostpath(maybe_hostpaths: &Option<Hostpaths>, path: AbsPathRef)
    -> Option<AbsPath>
{
    let hostpaths =
        if let Some(v) = maybe_hostpaths {
            v
        } else {
            return Some(path.to_vec());
        };

    let (prefix, host_dir) = hostpaths.value_at_prefix(path)?;

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
    abs_path_from_path_buf(Path::new(p))
}

#[derive(Debug, Snafu)]
enum NewAbsPathError {
    #[snafu(display("The absolute path was empty"))]
    EmptyAbsPath,
    // TODO We would ideally add the path component as a field on
    // `NoRootDirPrefix` and `SpecialComponentInAbsPath` to track the component
    // that was unexpected. However, the current version of `Snafu` being used
    // ["cannot use lifetime-parameterized errors as
    // sources"](https://github.com/shepmaster/snafu/issues/99), so we omit
    // this field for now.
    #[snafu(display("The absolute path didn't start with `/`"))]
    NoRootDirPrefix,
    #[snafu(display(
        "The absolute path contained a special component, such as `.` or `..`"
    ))]
    SpecialComponentInAbsPath,
}

fn abs_path_from_path_buf(p: &Path) -> Result<AbsPath, NewAbsPathError> {
    let mut components = p.components();

    let component = components.next()
        .context(EmptyAbsPath)?;

    if component != Component::RootDir {
        return Err(NewAbsPathError::NoRootDirPrefix);
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
        string += &path::MAIN_SEPARATOR.to_string();
        string += component.to_str()?;
    }

    Some(string)
}

fn abs_path_display_lossy(abs_path: AbsPathRef) -> String {
    if abs_path.is_empty() {
        return path::MAIN_SEPARATOR.to_string();
    }

    let mut string = String::new();
    for component in abs_path {
        string += &path::MAIN_SEPARATOR.to_string();
        if let Some(s) = component.to_str() {
            string += s;
        } else {
            string += &char::REPLACEMENT_CHARACTER.to_string();
        }
    }

    string
}

fn abs_path_extend(abs_path: &mut AbsPath, rel_path: RelPath) {
    abs_path.extend(rel_path);
}

type AbsPathRef<'a> = &'a [OsString];

type RelPath = Vec<OsString>;

/// Returns the `RelPath` derived from `p`. `p` must begin with a "current
/// directory" component (i.e. `.`).
fn rel_path_from_path_buf(p: &Path) -> Result<RelPath, NewRelPathError> {
    let mut components = p.components();

    let component = components.next()
        .context(EmptyRelPath)?;

    if component != Component::CurDir {
        return Err(NewRelPathError::NoCurDirPrefix);
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
    #[snafu(display("The relative path was empty"))]
    EmptyRelPath,
    // TODO See `NewAbsPathError` for more details on adding `Component` fields
    // in error variants.
    #[snafu(display("The relative path didn't start with `.`"))]
    NoCurDirPrefix,
    #[snafu(display(
        "The relative path contained a special component, such as `.` or `..`"
    ))]
    SpecialComponentInRelPath,
}

type RelPathRef<'a> = &'a [OsString];

fn path_buf_extend(path_buf: &mut PathBuf, rel_path: RelPathRef) {
    for component in rel_path {
        path_buf.push(component);
    }
}

fn shell(dock_file_name: &str, args: &ArgMatches) -> i32 {
    handle_run_with_extra_prefix_args(
        dock_file_name,
        args,
        to_strings(&[
            "--interactive",
            "--tty",
            "--entrypoint=/bin/sh",
        ]),
    )
}
