// Copyright 2021-2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::env;
use std::io;
use std::io::Write;
use std::process;
use std::process::ExitStatus;
use std::str;

use clap::Arg;
use clap::ArgMatches;
use clap::Command;

mod docker;
mod fs;
mod option;
mod canon_path;
mod rebuild;
mod run;
mod trie;

use run::RebuildForRunError;
use run::RunError;

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
        Command::new("dpnd")
            .version(env!("CARGO_PKG_VERSION"))
            .author(env!("CARGO_PKG_AUTHORS"))
            .about(env!("CARGO_PKG_DESCRIPTION"))
            .subcommands(vec![
                Command::new("rebuild")
                    .trailing_var_arg(true)
                    .about(rebuild_about)
                    .args(&[
                        Arg::new(TAGGED_IMG_FLAG)
                            .required(true)
                            .help("The tagged name for the new image")
                            .long_help(
                                "The tagged name for the new image, in the \
                                 form `name:tag`.",
                            ),
                        Arg::new(DOCKER_ARGS_FLAG)
                            .multiple_occurrences(true)
                            .help("Arguments to pass to `docker build`"),
                    ]),
                Command::new("run")
                    .trailing_var_arg(true)
                    .about(run_about)
                    .args(&[
                        Arg::new(ENV_FLAG)
                            .required(true)
                            .help("The environment to run"),
                        Arg::new(DOCKER_ARGS_FLAG)
                            .multiple_occurrences(true)
                            .help("Arguments to pass to `docker build`"),
                    ]),
                Command::new("shell")
                    .about(shell_about)
                    .args(&[
                        Arg::new(ENV_FLAG)
                            .help("The environment to run"),
                    ]),
            ])
            .get_matches();

    match args.subcommand() {
        Some(("rebuild", sub_args)) => {
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
        Some(("run", sub_args)) => {
            let exit_code = run(dock_file_name, sub_args);
            process::exit(exit_code);
        },
        Some(("shell", sub_args)) => {
            let exit_code = shell(dock_file_name, Some(sub_args));
            process::exit(exit_code);
        },
        Some((arg_name, sub_args)) => {
            // All subcommands defined in `args_defn` should be handled here,
            // so matching an unhandled command shouldn't happen.
            panic!(
                "unexpected command '{}' (arguments: '{:?}')",
                arg_name,
                sub_args,
            );
        },
        _ => {
            let exit_code = shell(dock_file_name, None);
            process::exit(exit_code);
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

fn run(dock_file_name: &str, args: &ArgMatches) -> i32 {
    let cmd_args =
        match args.values_of(DOCKER_ARGS_FLAG) {
            Some(vs) => vs.collect(),
            None => vec![],
        };

    handle_run(dock_file_name, Some(args), &[], &cmd_args)
}

fn handle_run(
    dock_file_name: &str,
    args: Option<&ArgMatches>,
    flags: &[&str],
    cmd_args: &[&str],
) -> i32 {
    let env_name = args.and_then(|a| a.value_of(ENV_FLAG));

    let result = run::run(dock_file_name, env_name, flags, cmd_args);
    match result {
        Ok(exit_status) => {
            exit_code_from_exit_status(exit_status)
        },
        Err(e) => {
            match e {
                RunError::RebuildForRunFailed{
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

fn shell(dock_file_name: &str, args: Option<&ArgMatches>) -> i32 {
    handle_run(
        dock_file_name,
        args,
        &[
            "--interactive",
            "--tty",
            "--entrypoint=/bin/sh",
        ],
        &[],
    )
}
