// Copyright 2021-2024 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::env;
use std::io;
use std::io::Error as IoError;
use std::io::StderrLock;
use std::io::StdoutLock;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process;
use std::process::ExitStatus;
use std::str;

use clap::Arg;
use clap::ArgMatches;
use clap::Command;

mod canon_path;
mod cmd_loggers;
mod docker;
mod fs;
mod init;
mod logging_process;
mod option;
mod rebuild;
mod run_in;
mod trie;

use cmd_loggers::CapturingCmdLogger;
use cmd_loggers::Prefixer;
use cmd_loggers::PrefixingCmdLogger;
use cmd_loggers::Stream;
use cmd_loggers::TimingPrefixingCmdLogger;
use init::FileAction;
use init::FileActionLogger;
use init::InitError;
use run_in::Args;
use run_in::CmdLoggers;
use run_in::Rebuild;
use run_in::RebuildAction;
use run_in::RebuildForRunInError;
use run_in::RunInError;

const DEFAULT_TEMPLATES_SOURCE: &str = env!("DOCK_DEFAULT_TEMPLATES_SOURCE");

const CACHE_TAG_FLAG: &str = "cache-tag";
const TAGGED_IMG_FLAG: &str = "tagged-image";
const COMMAND_ARGS_FLAG: &str = "docker-args";
const ENV_FLAG: &str = "env";
const DEBUG_FLAG: &str = "debug";
const TTY_FLAG: &str = "tty";
const SKIP_REBUILD_FLAG: &str = "skip-rebuild";
const SOURCE_FLAG: &str = "source";
const TEMPLATE_FLAG: &str = "template";

const DEFAULT_CACHE_TAG: &str = "cached";

#[allow(clippy::too_many_lines)]
fn main() {
    let dock_file_name = "dock.yaml";

    let rebuild_about: &str =
        "Replace a tagged Docker image with a new build";
    let run_about: &str = &format!(
        "Run a command in an environment defined in `{dock_file_name}`",
    );
    let shell_about: &str = &format!(
        "Start a shell in an environment defined in `{dock_file_name}`",
    );
    let init_about: &str =
        "Initialise the current directory with a Dock environment";
    let cache_tag_long_help: &str = &format!(
        "The tag to use for the image that will be replaced by the rebuild. \
         If an image with the tagged name `{TAGGED_IMG_FLAG}` exists then its \
         tag will be replaced by `{CACHE_TAG_FLAG}` for the duration of the \
         rebuild.",
    );

    let args =
        Command::new("dock")
            .version(env!("CARGO_PKG_VERSION"))
            .author(env!("CARGO_PKG_AUTHORS"))
            .about(env!("CARGO_PKG_DESCRIPTION"))
            .subcommands(vec![
                Command::new("rebuild")
                    .trailing_var_arg(true)
                    .about(rebuild_about)
                    .args(&[
                        Arg::new(CACHE_TAG_FLAG)
                            .long(CACHE_TAG_FLAG)
                            .default_value(DEFAULT_CACHE_TAG)
                            .help("The tag for the cache image")
                            .long_help(cache_tag_long_help),
                        Arg::new(TAGGED_IMG_FLAG)
                            .required(true)
                            .help("The tagged name for the new image")
                            .long_help(
                                "The tagged name for the new image, in the \
                                 form `name:tag`.",
                            ),
                        Arg::new(COMMAND_ARGS_FLAG)
                            .multiple_occurrences(true)
                            .help("Arguments to pass to `docker build`"),
                    ]),
                Command::new("run-in")
                    .trailing_var_arg(true)
                    .about(run_about)
                    .args(&[
                        Arg::new(CACHE_TAG_FLAG)
                            .long(CACHE_TAG_FLAG)
                            .default_value(DEFAULT_CACHE_TAG)
                            .help("The tag for the cache image")
                            .long_help(cache_tag_long_help),
                        Arg::new(DEBUG_FLAG)
                            .short('D')
                            .long(DEBUG_FLAG)
                            .help("Output debugging information"),
                        Arg::new(TTY_FLAG)
                            .short('T')
                            .long(TTY_FLAG)
                            .help("Allocate a pseudo-TTY"),
                        Arg::new(SKIP_REBUILD_FLAG)
                            .short('R')
                            .long(SKIP_REBUILD_FLAG)
                            .help("Don't rebuild before running"),
                        Arg::new(ENV_FLAG)
                            .required(true)
                            .help("The environment to run"),
                        Arg::new(COMMAND_ARGS_FLAG)
                            .multiple_occurrences(true)
                            .help("Arguments to pass to `docker run`"),
                    ]),
                Command::new("shell")
                    .about(shell_about)
                    .args(&[
                        // TODO Add support for `cache-tag` flag.
                        Arg::new(DEBUG_FLAG)
                            .short('D')
                            .long(DEBUG_FLAG)
                            .help("Output debugging information"),
                        Arg::new(SKIP_REBUILD_FLAG)
                            .short('R')
                            .long(SKIP_REBUILD_FLAG)
                            .help("Don't rebuild before running"),
                        Arg::new(ENV_FLAG)
                            .help("The environment to run"),
                    ]),
                Command::new("init")
                    .about(init_about)
                    .args(&[
                        // TODO Add support for debug flag.
                        Arg::new(SOURCE_FLAG)
                            .short('s')
                            .long(SOURCE_FLAG)
                            .default_value(DEFAULT_TEMPLATES_SOURCE)
                            .help("Use templates defined at this location"),
                        Arg::new(TEMPLATE_FLAG)
                            .required(true)
                            .help("The template to initialise with")
                            .long_help(
                                "Use the template with this name (from the \
                                 templates source) to initialise the current \
                                 project",
                            ),
                    ]),
            ])
            .get_matches();

    handle_arg_matches(&args, dock_file_name);
}

fn handle_arg_matches(args: &ArgMatches, dock_file_name: &str) {
    match args.subcommand() {
        Some(("rebuild", sub_args)) => {
            let docker_args =
                match sub_args.values_of(COMMAND_ARGS_FLAG) {
                    Some(vs) => vs.collect(),
                    None => vec![],
                };

            let exit_code = rebuild(
                sub_args.value_of(TAGGED_IMG_FLAG).unwrap(),
                sub_args.value_of(CACHE_TAG_FLAG).unwrap(),
                &docker_args,
            );
            process::exit(exit_code);
        },
        Some(("run-in", sub_args)) => {
            let exit_code = run_in(dock_file_name, sub_args);
            process::exit(exit_code);
        },
        Some(("shell", sub_args)) => {
            let exit_code = shell(dock_file_name, Some(sub_args));
            process::exit(exit_code);
        },
        Some(("init", sub_args)) => {
            let exit_code = init(dock_file_name, sub_args);
            process::exit(exit_code);
        },
        Some((arg_name, sub_args)) => {
            // All subcommands defined in `args_defn` should be handled here,
            // so matching an unhandled command shouldn't happen.
            panic!(
                "unexpected command '{arg_name}' (arguments: '{sub_args:?}')",
            );
        },
        _ => {
            let exit_code = shell(dock_file_name, None);
            process::exit(exit_code);
        },
    }
}

fn rebuild(target_img: &str, cache_tag: &str, docker_args: &[&str]) -> i32 {
    if let Some(i) = index_of_first_unsupported_flag(docker_args) {
        eprintln!("unsupported argument: `{}`", docker_args[i]);
        return 1;
    }

    let target_img_parts =
        target_img.split(':').collect::<Vec<&str>>();

    let img_name =
        if let [name, _tag] = target_img_parts.as_slice() {
            name
        } else {
            eprintln!(
                "`{TAGGED_IMG_FLAG}` must contain exactly one `:`",
            );
            return 1;
        };

    let cache_img = new_tagged_img_name(img_name, cache_tag);

    let rebuild_result = rebuild::rebuild_with_streaming_output(
        target_img,
        &cache_img,
        docker_args,
    );
    match rebuild_result {
        Ok(exit_status) => {
            exit_code_from_exit_status(exit_status)
        },
        Err(e) => {
            eprintln!("{e}");

            1
        },
    }
}

fn new_tagged_img_name(img_name: &str, tag: &str) -> String {
        format!("{img_name}:{tag}")
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

fn run_in(dock_file_name: &str, arg_matches: &ArgMatches) -> i32 {
    let cmd_args =
        match arg_matches.values_of(COMMAND_ARGS_FLAG) {
            Some(vs) => vs.collect(),
            None => vec![],
        };

    let mut docker_args = vec![];
    if arg_matches.is_present(TTY_FLAG) {
        docker_args.push("--tty");
    }

    let cache_tag = arg_matches.value_of(CACHE_TAG_FLAG).unwrap();

    let args = &Args{docker: &docker_args, command: &cmd_args};

    handle_run_in(dock_file_name, Some(arg_matches), args, None, cache_tag)
}

fn handle_run_in(
    dock_file_name: &str,
    arg_matches: Option<&ArgMatches>,
    args: &Args,
    shell: Option<PathBuf>,
    cache_tag: &str,
) -> i32 {
    let mut debug = false;
    let mut env_name = None;
    let mut rebuild_action = RebuildAction::Run;
    if let Some(args) = arg_matches {
        env_name = args.value_of(ENV_FLAG);

        if let Some(env) = env_name {
            if let Some(e) = env.strip_suffix("-env:") {
                env_name = Some(e);
            }
        }

        if args.is_present(DEBUG_FLAG) {
            debug = true;
        }

        if args.is_present(SKIP_REBUILD_FLAG) {
            rebuild_action = RebuildAction::Skip;
        }
    }

    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    let stderr = io::stderr();
    let mut stderr = stderr.lock();

    let mut logger =
        if debug {
            let logger = PrefixingCmdLogger::new(
                &mut stdout,
                b"[$] ",
                Prefixer::new(b"[>] "),
                Prefixer::new(b"[!] "),
            );
            let timing_logger = TimingPrefixingCmdLogger::new(logger, b"[@] ");

            CmdLoggers::Debugging(timing_logger)
        } else {
            CmdLoggers::Capturing(CapturingCmdLogger::new())
        };

    let result = run_in::run_in(
        &mut logger,
        dock_file_name,
        env_name,
        &Rebuild{action: rebuild_action, cache_tag: cache_tag.to_string()},
        args,
        shell,
    );

    // TODO Check if the prefixing command logger has an error.

    match result {
        Ok(exit_status) => {
            exit_code_from_exit_status(exit_status)
        },
        Err(err) => {
            match (err, logger) {
                (
                    RunInError::RebuildForRunInFailed{
                        source: RebuildForRunInError::RebuildUnsuccessful{..},
                    },
                    CmdLoggers::Capturing(logger),
                ) => {
                    let chunks = &logger.chunks;

                    write_streams(&mut stdout, &mut stderr, chunks);
                },
                (e, _) => {
                    eprintln!("{e}");
                },
            };

            1
        },
    }
}

fn write_streams(
    mut stdout: &mut StdoutLock,
    mut stderr: &mut StderrLock,
    chunks: &[(Stream, Vec<u8>)],
) {
    for (stream, bs) in chunks {
        let out =
            match stream {
                Stream::Stdout => &mut stdout as &mut dyn Write,
                Stream::Stderr => &mut stderr as &mut dyn Write,
            };

        if let Err(e) = out.write_all(bs) {
            eprintln!("couldn't write stream ({stream:?}): {e}");
            return;
        }
    }

    if let Err(e) = stdout.flush() {
        eprintln!("couldn't flush STDOUT: {e}");
    }

    if let Err(e) = stderr.flush() {
        eprintln!("couldn't flush STDERR: {e}");
    }
}

fn shell(dock_file_name: &str, args: Option<&ArgMatches>) -> i32 {
    handle_run_in(
        dock_file_name,
        args,
        &Args{
            // TODO Add tests for `--network=host`.
            docker: &["--interactive", "--tty", "--network=host"],
            command: &[],
        },
        Some(Path::new("/bin/sh").to_path_buf()),
        DEFAULT_CACHE_TAG,
    )
}

fn init(dock_file_name: &str, args: &ArgMatches) -> i32 {
    let raw_source = args.value_of(SOURCE_FLAG).unwrap();
    let source =
        match init::parse_templates_source(raw_source) {
            Ok(source) => {
                source
            },
            Err(e) => {
                eprintln!("{e}");
                return 1;
            },
        };

    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    let mut logger = WriterFileActionLogger{w: &mut stdout};
    let template = args.value_of(TEMPLATE_FLAG).unwrap();
    let dock_file = PathBuf::from(dock_file_name);
    let result = init::init(
        &mut logger,
        &source,
        template,
        &dock_file,
        Path::new("."),
    );
    if let Err(e) = result {
        match e {
            InitError::DockFileAlreadyExists => {
                eprintln!(
                    "The current directory already contains '{}'",
                    dock_file.display(),
                );
                return 2;
            },
            e => {
                eprintln!("{e}");
                return 1;
            },
        }
    }

    0
}

struct WriterFileActionLogger<'a> {
    w: &'a mut dyn Write,
}

impl<'a> FileActionLogger for WriterFileActionLogger<'a> {
    fn log_file_action(&mut self, file: &Path, action: FileAction)
        -> Result<(), IoError>
    {
        let msg =
            match action {
                FileAction::Create => "Created",
                FileAction::Skip => "Skipped",
            };

        writeln!(self.w, "{} '{}'", msg, file.display())
    }
}
