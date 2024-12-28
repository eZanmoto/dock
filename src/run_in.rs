// Copyright 2022-2024 Sean Kelleher. All rights reserved.
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
use std::io::Error as IoError;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Output;
use std::process::Stdio;
use std::str;
use std::str::Utf8Error;

use serde::Deserialize;
use serde_yaml::Error as SerdeYamlError;
use serde_yaml::Value;
use snafu::OptionExt;
use snafu::ResultExt;
use snafu::Snafu;

use crate::canon_path::AbsPath;
use crate::canon_path::NewAbsPathError;
use crate::canon_path::NewRelPathError;
use crate::canon_path::RelPath;
use crate::cmd_loggers::CapturingCmdLogger;
use crate::cmd_loggers::TimingPrefixingCmdLogger;
use crate::fs;
use crate::fs::FindAndOpenFileError;
use crate::logging_process;
use crate::logging_process::CmdLoggerMsg;
use crate::logging_process::CommandLogger;
use crate::logging_process::RunError as LoggingProcessRunError;
use crate::option::OptionResultExt;
use crate::rebuild;
use crate::rebuild::DockerContext;
use crate::rebuild::RebuildError;
use crate::spinner;
use crate::spinner::SpinError;
use crate::trie::InsertError;
use crate::trie::Trie;

#[derive(Deserialize)]
pub struct DockConfig {
    pub schema_version: String,
    pub organisation: String,
    pub project: String,
    pub default_shell_env: String,
    pub environments: HashMap<String, DockEnvironmentConfig>
}

// TODO Consider whether to automatically deserialise `PathBuf`s using `serde`,
// or to read them as `String`s and parse them directly.
#[derive(Deserialize)]
pub struct DockEnvironmentConfig {
    pub context: Option<PathBuf>,
    pub workdir: Option<String>,
    pub build_args: Option<Vec<String>>,
    pub run_args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub cache_volumes: Option<HashMap<String, PathBuf>>,
    pub mounts: Option<HashMap<PathBuf, PathBuf>>,
    pub mount_local: Option<Vec<DockEnvironmentMountLocalConfig>>,
    pub shell: Option<PathBuf>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DockEnvironmentMountLocalConfig {
    User,
    Group,
    ProjectDir,
    Docker,
}

pub enum CmdLoggers<'a> {
    Debugging(TimingPrefixingCmdLogger<'a>),
    Capturing(CapturingCmdLogger),
}

impl CommandLogger for CmdLoggers<'_> {
    fn log(&mut self, msg: CmdLoggerMsg) {
        match self {
            Self::Debugging(logger) => {
                logger.log(msg);
            },
            Self::Capturing(logger) => {
                logger.log(msg);
            },
        }
    }
}

pub fn run_in(
    // NOTE We would ideally take `logger` as `dyn CommandLogger`, but this
    // type can't be shared between threads safely, which is required by
    // `spinner::spin`.
    logger: &mut dyn CommandLogger,
    dock_file_name: &str,
    maybe_env_name: Option<&str>,
    rebuild: &Rebuild,
    args: &Args,
    // TODO Remove the `shell` parameter to decouple this function from the
    // `shell` subcommand.
    shell: Option<PathBuf>,
    show_rebuild_spinner: bool,
) -> Result<ExitStatus, RunInError> {
    let (dock_dir, conf) = find_and_parse_dock_config(dock_file_name)
        .context(FindAndParseDockConfigFailed{dock_file_name})?;

    let env_name = maybe_env_name.unwrap_or(&conf.default_shell_env);

    let env = conf.environments.get(env_name)
        .context(EnvironmentNotFound{name: env_name})?;

    let img_name = image_name(&conf.organisation, &conf.project, env_name);
    let target_img = img_name.clone() + ":latest";
    let cache_img = img_name + ":" + &rebuild.cache_tag;

    if let RebuildAction::Run = rebuild.action {
        let env_context =
            env.context
                .as_ref()
                .and_maybe_then(|path| RelPath::try_from(path.clone()))
                .context(RelPathFromContextPathFailed)?;

        let build_args = env.build_args.clone().unwrap_or_default();

        let build_args: Vec<&str> =
            build_args
                .iter()
                .map(AsRef::as_ref)
                .collect();

        let mut rebuild = || rebuild_for_run_in(
            logger,
            &dock_dir,
            env_name,
            env_context.as_ref(),
            &target_img,
            &cache_img,
            &build_args,
        );

        if show_rebuild_spinner {
            let rebuild_msg = format!("Rebuilding '{target_img}'");
            spinner::spin(
                rebuild_msg,
                rebuild,
            )
                .context(SpinFailed)?
                .context(SpinnerRebuildForRunInFailed)?;
        } else {
            rebuild()
                .context(RebuildForRunInFailed)?;
        }
    }

    let vol_name_prefix =
        cache_vol_name_prefix(&conf.organisation, &conf.project, env_name);

    let mut run_args = to_strings(&["run"]);

    if let Some(mut shell) = shell {
        if let Some(s) = &env.shell {
            shell.clone_from(s);
        }

        run_args.push(format!("--entrypoint={}", shell.display()));
    }

    let main_run_args =
        prepare_run_in_args(
            logger,
            env,
            &dock_dir,
            &vol_name_prefix,
            &target_img,
        )
            .context(PrepareRunInArgsFailed)?;

    run_args.extend(main_run_args);

    run_args.extend(to_strings(args.docker));

    run_args.push(target_img);

    run_args.extend(to_strings(args.command));

    // TODO Perform the side effects of `prepare_run_cache_volumes_args` here.

    let prog = OsStr::new("docker");
    let args: Vec<&OsStr> =
        run_args
            .iter()
            .map(OsStr::new)
            .collect();

    let mut cmd_line = vec![prog];
    cmd_line.extend(&args);
    logger.log(CmdLoggerMsg::Cmd(&cmd_line));

    let mut cmd = Command::new(prog);
    let err = cmd.args(&args).exec();

    Err(RunInError::ExecFailed{source: err})
}

pub struct Args<'a> {
    pub docker: &'a [&'a str],
    pub command: &'a [&'a str],
}

pub struct Rebuild {
    pub action: RebuildAction,
    pub cache_tag: String,
}

pub enum RebuildAction {
    Run,
    Skip,
}

// TODO The following variants don't need to contain `dock_file_name` as a
// field because it's passed to the `run_with_extra_prefix_args`, but we
// include it for now for simplicity.
#[derive(Debug, Snafu)]
pub enum RunInError {
    #[snafu(display(
        "Couldn't find and parse '{}': {}",
        dock_file_name,
        source,
    ))]
    FindAndParseDockConfigFailed{
        source: FindAndParseDockConfigError,
        dock_file_name: String,
    },
    #[snafu(display("Dock environment '{}' isn't defined", name))]
    EnvironmentNotFound{name: String},
    #[snafu(display(
        "Couldn't get path to the context directory as a relative path: {}",
        source,
    ))]
    RelPathFromContextPathFailed{source: NewRelPathError},
    #[snafu(display("{}", source))]
    SpinFailed{source: SpinError},
    #[snafu(display("{}", source))]
    SpinnerRebuildForRunInFailed{source: RebuildForRunInError},
    #[snafu(display("{}", source))]
    RebuildForRunInFailed{source: RebuildForRunInError},
    #[snafu(display(
        "Couldn't prepare arguments for `docker run`: {}",
        source,
    ))]
    PrepareRunInArgsFailed{source: PrepareRunInArgsError},
    #[snafu(display("`exec` failed: {}", source))]
    ExecFailed{source: IoError},
}

pub fn image_name(org: &str, proj: &str, env_name: &str) -> String {
    format!("{org}/{proj}.{env_name}")
}

pub fn find_and_parse_dock_config(dock_file_name: &str)
    -> Result<(AbsPath, DockConfig), FindAndParseDockConfigError>
{
    let cwd = env::current_dir()
        .context(GetCurrentDirFailed)?;

    let (dock_dir, conf_reader) = fs::find_and_open_file(&cwd, dock_file_name)
        .context(OpenDockFileFailed)?
        .context(DockFileNotFound)?;

    let conf = parse_dock_config(conf_reader)
        .context(ParseDockConfigFailed)?;

    if !conf.environments.contains_key(&conf.default_shell_env) {
        let env = conf.default_shell_env;
        return Err(FindAndParseDockConfigError::DefaultShellEnvMissing{env});
    }

    for env in conf.environments.keys() {
        let position = env.chars().position(|c| !is_env_name_char(c));
        if let Some(pos) = position {
            let name = env.to_string();
            let e = FindAndParseDockConfigError::InvalidEnvName{name, pos};
            return Err(e);
        }
    }

    let dock_dir = AbsPath::try_from(dock_dir.clone())
        .context(DockDirAsAbsPathFailed{dock_dir})?;

    Ok((dock_dir, conf))
}

fn is_env_name_char(c: char) -> bool {
    c == '.' || c == '_' || c.is_ascii_lowercase() || c.is_ascii_digit()
}

#[derive(Debug, Snafu)]
pub enum FindAndParseDockConfigError {
    #[snafu(display("Couldn't get the current directory: {}", source))]
    GetCurrentDirFailed{source: IoError},
    #[snafu(display("Couldn't find Dock file"))]
    DockFileNotFound,
    #[snafu(display("Couldn't open: {}", source))]
    OpenDockFileFailed{source: FindAndOpenFileError},
    #[snafu(display("Couldn't parse: {}", source))]
    ParseDockConfigFailed{source: ParseDockConfigError},
    #[snafu(display("`default_shell_env` '{}' isn't defined", env))]
    DefaultShellEnvMissing{env: String},
    #[snafu(display(
        "Couldn't get path to current Dock directory ('{}') as an absolute \
            path: {}",
        dock_dir.display(),
        source,
    ))]
    DockDirAsAbsPathFailed{source: NewAbsPathError, dock_dir: PathBuf},
    #[snafu(display(
        "Invalid character '{}' at position {} in environment name '{}' \
            (environment names may only contain periods, underscores, ASCII \
            digits, and lowercase ASCII letters)",
        name
            .chars()
            .nth(*pos)
            .unwrap(),
        pos,
        name,
    ))]
    InvalidEnvName{name: String, pos: usize},
}

fn parse_dock_config(file: File) -> Result<DockConfig, ParseDockConfigError> {
    let conf_value: Value = serde_yaml::from_reader(file)
        .context(ParseYamlFailed)?;

    let vsn = conf_value.get("schema_version")
        .context(MissingSchemaVersion)?;

    if vsn != "0.1" {
        // TODO Add `vsn` to the error context.
        return Err(ParseDockConfigError::UnsupportedSchemaVersion);
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

fn rebuild_for_run_in(
    logger: &mut dyn CommandLogger,
    dock_dir: &AbsPath,
    env_name: &str,
    maybe_context_sub_path: Option<&RelPath>,
    img: &str,
    cache_img: &str,
    args: &[&str],
)
    -> Result<(), RebuildForRunInError>
{
    // TODO Consider the fact that `env_name` may contain `/`; it may be worth
    // adding an `EnvName` type with validation in its constructor.
    let dockerfile_name = OsString::from(format!("{env_name}.Dockerfile"));

    let dockerfile_path =
        dock_dir.concat(&rel_path_from_component(dockerfile_name));

    let docker_context = new_docker_context(
        dock_dir,
        dockerfile_path,
        maybe_context_sub_path,
    )
        .context(NewDockerRebuildInputFailed)?;

    let status = rebuild::rebuild(logger, img, cache_img, docker_context, args)
        .context(RebuildFailed{img: img.to_string()})?;

    if !status.success() {
        let img = img.to_string();

        return Err(RebuildForRunInError::RebuildUnsuccessful{img});
    }

    // We ignore the status code returned "by the build step" because there
    // isn't anything to distinguish it from a status code returned "by the run
    // step".
    Ok(())
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
pub enum RebuildForRunInError {
    #[snafu(display("Couldn't prepare input for `dock rebuild`: {}", source))]
    NewDockerRebuildInputFailed{source: NewDockerContextError},
    #[snafu(display("Couldn't rebuild '{}': {}", img, source))]
    RebuildFailed{
        source: RebuildError<ExitStatus, LoggingProcessRunError>,
        img: String,
    },
    #[snafu(display("Rebuild of '{}' returned an unsuccessful status", img))]
    RebuildUnsuccessful{img: String},
}

fn rel_path_from_component(c: OsString) -> RelPath {
    RelPath::from(vec![c])
}

fn new_docker_context(
    dock_dir: &AbsPath,
    dockerfile_path: AbsPath,
    maybe_context_sub_path: Option<&RelPath>,
)
    -> Result<DockerContext, NewDockerContextError>
{
    if let Some(context_sub_path) = maybe_context_sub_path {
        let context_path = dock_dir.concat(context_sub_path);

        Ok(DockerContext::Dir{path: context_path, dockerfile: dockerfile_path})
    } else {
        let dockerfile = File::open(PathBuf::from(dockerfile_path.clone()))
            .context(OpenDockerfileFailed{path: dockerfile_path})?;

        Ok(DockerContext::Empty{dockerfile})
    }
}

#[derive(Debug, Snafu)]
pub enum NewDockerContextError {
    #[snafu(display(
        "Couldn't open the Dockerfile '{}': {}",
        path.display_lossy(),
        source,
    ))]
    OpenDockerfileFailed{source: IoError, path: AbsPath},
}

fn prepare_run_in_args(
    logger: &mut dyn CommandLogger,
    env: &DockEnvironmentConfig,
    dock_dir: &AbsPath,
    vol_name_prefix: &str,
    target_img: &str,
)
    -> Result<Vec<String>, PrepareRunInArgsError>
{
    // We pass `--init` in order to forward signals and reap processes. TODO
    // Give concrete examples to justify providing `--init`.
    // TODO Add tests for `--init`.
    let mut run_args = to_strings(&["--rm", "--init"]);

    if let Some(cache_volumes) = &env.cache_volumes {
        let args = prepare_run_cache_volumes_args(
            logger,
            cache_volumes,
            vol_name_prefix,
            target_img,
        )
            .context(PrepareRunInCacheVolumesArgsFailed)?;

        run_args.extend(args);
    }

    run_args.extend(env.run_args.clone().unwrap_or_default());

    if let Some(dir) = &env.workdir {
        run_args.push(format!("--workdir={dir}"));
    }

    if let Some(env_vars) = &env.env {
        for (k, v) in env_vars {
            run_args.push(format!("--env={k}={v}"));
        }
    }

    // TODO Add tests for nested mounting.
    let mut parsed_mounts = vec![];
    if let Some(mounts) = &env.mounts {
        for (rel_outer_path, inner_path) in mounts {
            let rel_outer_path =
                RelPath::try_from(rel_outer_path.clone())
                    .context(ParseConfigOuterPathFailed{
                        rel_outer_path,
                        inner_path,
                    })?;

            parsed_mounts.push((rel_outer_path, inner_path.clone()));
        }
    }

    if let Some(mount_local) = &env.mount_local {
        // TODO Add tests for nested mounting of the project directory.
        if mount_local.contains(&DockEnvironmentMountLocalConfig::ProjectDir) {
            let raw_workdir = env.workdir.as_ref()
                .context(WorkdirNotSet)?;

            let cur_dir = RelPath::from(vec![]);
            let workdir = PathBuf::from(raw_workdir);

            parsed_mounts.push((cur_dir, workdir));
        }

        let args = prepare_mount_local_run_args(mount_local)
            .context(PrepareRunInMountLocalArgsFailed)?;

        run_args.extend(args);
    }

    if !parsed_mounts.is_empty() {
        let cur_hostpaths = hostpaths()
            .context(GetHostpathsFailed)?;

        let args = prepare_run_mount_args(
            dock_dir,
            &parsed_mounts,
            cur_hostpaths.as_ref(),
        )
            .context(PrepareRunInMountArgsFailed)?;

        run_args.extend(args);
    }

    Ok(run_args)
}

const DOCKER_SOCK_PATH: &str = "/var/run/docker.sock";

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
pub enum PrepareRunInArgsError {
    #[snafu(display(
        "Couldn't prepare cache volume arguments for `docker run`: {}",
        source,
    ))]
    PrepareRunInCacheVolumesArgsFailed{
        source: PrepareRunInCacheVolumesArgsError,
    },
    #[snafu(display("`workdir` is required when `project_dir` is mounted"))]
    WorkdirNotSet,
    #[snafu(display(
        "Couldn't prepare \"local mount\" arguments for `docker run`: {}",
        source,
    ))]
    PrepareRunInMountLocalArgsFailed{source: PrepareRunInMountLocalArgsError},
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
    PrepareRunInMountArgsFailed{source: PrepareRunInMountArgsError},
}

// TODO This method doesn't just prepare the cache volume arguments for the
// `docker run` command, but also creates the volumes (if they don't exist) and
// changes their permissions. This responsibility should ideally be moved to a
// dedicated function of its own.
fn prepare_run_cache_volumes_args(
    logger: &mut dyn CommandLogger,
    cache_volumes: &HashMap<String, PathBuf>,
    vol_name_prefix: &str,
    target_img: &str,
)
    -> Result<Vec<String>, PrepareRunInCacheVolumesArgsError>
{
    let mut args = vec![];

    for (name, path) in cache_volumes {
        let path_abs_path = AbsPath::try_from(path.clone())
            .context(CacheVolDirAsAbsPathFailed)?;

        let path_cli_arg = path_abs_path.display()
            .context(RenderCacheVolDirFailed{dir: path_abs_path})?;

        let vol_name = cache_vol_name(vol_name_prefix, name);
        let mount_spec =
            format!("type=volume,src={vol_name},dst={path_cli_arg}");
        let mount_arg = format!("--mount={mount_spec}");

        args.push(mount_arg.clone());

        let prog = OsStr::new("docker");
        let raw_inspect_args = &["volume", "inspect", vol_name.as_str()];
        let inspect_args = new_os_strs(raw_inspect_args);
        let status =
            logging_process::run(logger, prog, &inspect_args, Stdio::null())
                .context(CheckCacheExistenceFailed{
                    vol_name: vol_name.clone(),
                })?;

        if status.success() {
            // `vol_name` already exists, so we skip creating and initialising
            // it.
            continue;
        }

        let raw_docker_args = &[
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
        ];
        let docker_args = new_os_strs(raw_docker_args);
        logging_process::run(logger, prog, &docker_args, Stdio::null())
            .context(ChangeCacheOwnershipFailed{vol_name})?;
    }

    Ok(args)
}

pub fn cache_vol_name_prefix(org: &str, proj: &str, env_name: &str) -> String {
    format!("{org}.{proj}.{env_name}")
}

pub fn cache_vol_name(vol_name_prefix: &str, name: &str) -> String {
    format!("{vol_name_prefix}.cache.{name}")
}

pub fn new_os_strs<'a>(strs: &'a [&'a str]) -> Vec<&'a OsStr> {
    strs
        .iter()
        .map(OsStr::new)
        .collect()
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
pub enum PrepareRunInCacheVolumesArgsError {
    #[snafu(display(
        "Couldn't convert the cache volume directory to an absolute path: {}",
        source,
    ))]
    CacheVolDirAsAbsPathFailed{source: NewAbsPathError},
    #[snafu(display(
        "Couldn't render the cache volume directory (lossy rendering: '{}')",
        dir.display_lossy(),
    ))]
    RenderCacheVolDirFailed{dir: AbsPath},
    #[snafu(display(
        "Couldn't check whether the cache volume '{}' exists: {}",
        vol_name,
        source,
    ))]
    CheckCacheExistenceFailed{
        vol_name: String,
        source: LoggingProcessRunError,
    },
    #[snafu(display(
        "Couldn't set the ownership of the cache volume '{}': {}",
        vol_name,
        source,
    ))]
    ChangeCacheOwnershipFailed{
        vol_name: String,
        source: LoggingProcessRunError
    },
}

fn prepare_mount_local_run_args(
    mount_local: &[DockEnvironmentMountLocalConfig],
)
    -> Result<Vec<String>, PrepareRunInMountLocalArgsError>
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
            args.push(format!("--user={user_group}"));
        } else {
            args.push(format!("--user={}", user_id.trim_end()));
        }
    } else if mount_local.contains(&DockEnvironmentMountLocalConfig::Group) {
        return Err(PrepareRunInMountLocalArgsError::GroupMountedWithoutUser);
    }

    if mount_local.contains(&DockEnvironmentMountLocalConfig::Docker) {
        let meta = std_fs::metadata(DOCKER_SOCK_PATH)
            .context(GetDockerSockMetadataFailed)?;

        let mount_spec = format!(
            "type=bind,src={DOCKER_SOCK_PATH},dst={DOCKER_SOCK_PATH}",
        );
        args.extend(to_strings(&[
            &format!("--mount={mount_spec}"),
            &format!("--group-add={}", meta.gid()),
        ]));
    }

    Ok(args)
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
pub enum PrepareRunInMountLocalArgsError {
    #[snafu(display("Couldn't get user ID for the active user: {}", source))]
    GetUserIdFailed{source: RunCommandError},
    #[snafu(display("Couldn't get group ID for the active user: {}", source))]
    GetGroupIdFailed{source: RunCommandError},
    #[snafu(display("local `group` was mounted without `user`"))]
    GroupMountedWithoutUser,
    #[snafu(display("Couldn't get metadata for Docker socket: {}", source))]
    GetDockerSockMetadataFailed{source: IoError},
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
pub enum RunCommandError {
    #[snafu(display("Couldn't run the command: {}", source))]
    AssertRunFailed{source: AssertRunError},
    #[snafu(display("Couldn't convert STDOUT to UTF-8: {}", source))]
    ConvertStdoutToUtf8Failed{source: Utf8Error, stdout_bytes: Vec<u8>},
}

pub fn assert_run<I, S>(prog: &str, args: I) -> Result<Output, AssertRunError>
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
    dock_dir: &AbsPath,
    mounts: &[(RelPath, PathBuf)],
    cur_hostpaths: Option<&Hostpaths>,
)
    -> Result<Vec<String>, PrepareRunInMountArgsError>
{
    let mut hostpath_cli_args = vec![];
    for (rel_outer_path, inner_path) in mounts {
        let mut path = dock_dir.concat(rel_outer_path);

        // TODO Add `cur_hostpaths` to the error context. This ideally requires
        // `&Trie` to implement `Clone` so that a new, owned copy of
        // `cur_hostpaths` can be added to the error.
        path = apply_hostpath(cur_hostpaths, &path)
            .context(NoPathRouteOnHost{attempted_path: path})?;

        let host_path_cli_arg = path.display()
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
                    let e =
                        PrepareRunInMountArgsError::InnerPathAsCliArgFailed{
                            path,
                        };

                    return Err(e);
                },
            };

        hostpath_cli_args.push((host_path_cli_arg, inner_path_cli_arg));
    }

    let mut args = vec![];

    for (host_path, inner_path) in &hostpath_cli_args {
        let mount_spec =
            format!("type=bind,src={host_path},dst={inner_path}");

        args.push(format!("--mount={mount_spec}"));
    }

    let rendered_hostpaths = hostpath_cli_args
        .into_iter()
        .map(|(host_path, inner_path)| format!("{host_path}:{inner_path}"))
        .collect::<Vec<String>>()
        .join(":");

    args.push(
        format!("--env={DOCK_HOSTPATHS_VAR_NAME}={rendered_hostpaths}")
    );

    Ok(args)
}

#[derive(Debug, Snafu)]
pub enum PrepareRunInMountArgsError {
    #[snafu(display(
        "No route to the path '{}' was found on the host",
        attempted_path.display_lossy(),
    ))]
    NoPathRouteOnHost{attempted_path: AbsPath},
    #[snafu(display(
        "Couldn't render the hostpath mapping to '{}' (lossy rendering: '{}')",
        inner_path.display(),
        path.display_lossy(),
    ))]
    RenderHostPathFailed{path: AbsPath, inner_path: PathBuf},
    #[snafu(display(
        "Couldn't render the inner path '{}' as a CLI argument",
        PathBuf::from(path).display(),
    ))]
    InnerPathAsCliArgFailed{path: OsString},
}

const DOCK_HOSTPATHS_VAR_NAME: &str = "DOCK_HOSTPATHS";

#[derive(Debug)]
struct Hostpaths {
    host_paths: Trie<OsString, AbsPath>,
}

impl Hostpaths {
    fn new() -> Hostpaths {
        Self{host_paths: Trie::new()}
    }

    fn insert(&mut self, outer_path: AbsPath, inner_path: AbsPath)
        -> Result<(), HostpathInsertError>
    {
        match self.host_paths.insert(&inner_path, outer_path.clone()) {
            Ok(()) => {
                Ok(())
            },
            Err(err) => {
                let e =
                    match err {
                        InsertError::EmptyKey =>
                            // TODO These parameters can be added at a higher
                            // level.
                            HostpathInsertError::EmptyInnerPath{outer_path},
                        InsertError::PrefixContainsValue =>
                            HostpathInsertError::InnerPathAncestorHasMapping{
                                outer_path,
                                inner_path,
                            },
                        InsertError::DirAtKey =>
                            HostpathInsertError::InnerPathDescendentHasMapping{
                                outer_path,
                                inner_path,
                            },
                    };

                Err(e)
            },
        }
    }

    fn lookup(&self, path: &AbsPath) -> Option<AbsPath> {
        let (prefix, host_dir) = self.host_paths.value_at_prefix(path)?;

        let rel_path: Vec<OsString> =
            path
                .iter()
                .skip(prefix.len())
                .cloned()
                .collect();

        let host_path = host_dir.concat(&RelPath::from(rel_path));

        Some(host_path)
    }
}

#[derive(Debug, Snafu)]
pub enum HostpathInsertError {
    #[snafu(display(
        "The path '{}' maps to an empty path",
        outer_path.display_lossy(),
    ))]
    EmptyInnerPath{outer_path: AbsPath},
    #[snafu(display(
        "A host path maps to an ancestor of '{}' (which is mapped-to by '{}')",
        inner_path.display_lossy(),
        outer_path.display_lossy(),
    ))]
    InnerPathAncestorHasMapping{outer_path: AbsPath, inner_path: AbsPath},
    #[snafu(display(
        "A host path maps to a descendant of '{}' (which is mapped-to by \
            '{}')",
        inner_path.display_lossy(),
        outer_path.display_lossy(),
    ))]
    InnerPathDescendentHasMapping{outer_path: AbsPath, inner_path: AbsPath},
}

impl TryFrom<Vec<(&str, &str)>> for Hostpaths {
    type Error = HostpathFromPairsError;

    fn try_from(pairs: Vec<(&str, &str)>) -> Result<Self, Self::Error> {
        let mut hps = Hostpaths::new();

        for (outer_path, inner_path) in pairs {
            let abs_outer_path = AbsPath::parse(outer_path)
                .context(ParseOuterPathFailed{
                    outer_path: (*outer_path).to_string(),
                    inner_path: (*inner_path).to_string(),
                })?;

            let abs_inner_path = AbsPath::parse(inner_path)
                .context(ParseInnerPathFailed{
                    outer_path: (*outer_path).to_string(),
                    inner_path: (*inner_path).to_string(),
                })?;

            hps.insert(abs_outer_path, abs_inner_path)
                .context(HostpathInsertFailed{
                    outer_path: (*outer_path).to_string(),
                    inner_path: (*inner_path).to_string(),
                })?;
        }

        Ok(hps)
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
pub enum HostpathFromPairsError {
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
        "Couldn't parse '{}' as an absolute path (mapped from '{}'): {}",
        outer_path,
        inner_path,
        source,
    ))]
    ParseInnerPathFailed{
        source: NewAbsPathError,
        outer_path: String,
        inner_path: String,
    },
    #[snafu(display(
        "Couldn't add hostpath mapping '{}' to '{}' to hostpaths: {}",
        inner_path,
        outer_path,
        source,
    ))]
    HostpathInsertFailed{
        source: HostpathInsertError,
        outer_path: String,
        inner_path: String,
    },
}

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

    let pairs = pairs(&raw_hostpaths)
        .context(UnmatchedHostpath{hostpaths: to_strings(&raw_hostpaths) })?;

    let hostpaths = Hostpaths::try_from(pairs)
        .context(CreateHostpathsFailed)?;

    Ok(Some(hostpaths))
}

#[derive(Debug, Snafu)]
pub enum HostpathsError {
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
        "Couldn't create hostpaths from '${}': {}",
        DOCK_HOSTPATHS_VAR_NAME,
        source,
    ))]
    CreateHostpathsFailed{source: HostpathFromPairsError},
}

fn pairs<'a, T: Debug + ?Sized>(xs: &[&'a T]) -> Option<Vec<(&'a T, &'a T)>> {
    if xs.len() % 2 == 1 {
        return None;
    }

    let mut pairs = Vec::with_capacity(xs.len() / 2);

    for pair in xs.chunks(2) {
        if let [a, b] = pair {
            pairs.push((*a, *b));
        } else {
            // `chunks(2)` should always return slices of length 2.
            panic!("chunk didn't have length 2: {pair:?}");
        }
    }

    Some(pairs)
}

fn apply_hostpath(maybe_hostpaths: Option<&Hostpaths>, path: &AbsPath)
    -> Option<AbsPath>
{
    if let Some(hps) = maybe_hostpaths {
        hps.lookup(path)
    } else {
        Some(path.clone())
    }
}
