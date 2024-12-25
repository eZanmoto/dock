// Copyright 2021-2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use crate::test_setup;
use crate::test_setup::Definition;

use crate::assert_cmd::Command as AssertCommand;

#[test]
// Given (1) the Dockerfile contains the `false` command
// When the `rebuild` subcommand is run
// Then (A) the command is not successful
fn failing_dockerfile_returns_non_zero() {
    let test = test_setup::assert_apply(&Definition{
        name: "failing_dockerfile_returns_non_zero",
        dockerfile_steps: indoc!{"
            RUN false
        "},
        fs: &hashmap!{},
    });
    let mut cmd = new_test_cmd(test.dir, &test.image_tagged_name);

    let cmd_result = cmd.assert();

    // (A)
    cmd_result.failure();
}

// TODO Duplicated from `success.rs`.
fn new_test_cmd(
    root_test_dir: String,
    image_tagged_name: &str,
) -> AssertCommand {
    let mut cmd = AssertCommand::cargo_bin(env!("CARGO_PKG_NAME"))
        .expect("couldn't create command for package binary");
    cmd.args(vec!["rebuild", image_tagged_name, "."]);
    cmd.current_dir(root_test_dir);
    cmd.env_clear();
    // We set `HOME` because if unset then Docker BuildKit will create a
    // `.docker` directory in the working directory during builds.
    cmd.env("HOME", env!("HOME"));

    cmd
}

#[test]
// Given a valid Dockerfile
// When the `rebuild` subcommand is run with a `-t` argument
// Then (A) the command fails
//     AND (B) the command STDOUT is empty
//     AND (C) the command STDERR contains an error message
fn short_tag_argument() {
    // (1)
    let test = test_setup::assert_apply(&Definition{
        name: "short_tag_argument",
        dockerfile_steps: "",
        fs: &hashmap!{},
    });
    let mut cmd = new_test_cmd(test.dir, &test.image_tagged_name);
    let flag = "-t";
    cmd.args(&[flag, &test.image_tagged_name]);

    let cmd_result = cmd.assert();

    cmd_result
        // (A)
        .failure()
        // (B)
        .stdout("")
        // (C)
        .stderr(format!("unsupported argument: `{}`\n", flag));
}

#[test]
// Given a valid Dockerfile
// When the `rebuild` subcommand is run with a `--tag` argument
// Then (A) the command fails
//     AND (B) the command STDOUT is empty
//     AND (C) the command STDERR contains an error message
fn long_tag_argument() {
    // (1)
    let test = test_setup::assert_apply(&Definition{
        name: "long_tag_argument",
        dockerfile_steps: "",
        fs: &hashmap!{},
    });
    let mut cmd = new_test_cmd(test.dir, &test.image_tagged_name);
    let flag = "--tag";
    cmd.args(&[flag, &test.image_tagged_name]);

    let cmd_result = cmd.assert();

    cmd_result
        // (A)
        .failure()
        // (B)
        .stdout("")
        // (C)
        .stderr(format!("unsupported argument: `{}`\n", flag));
}

#[test]
// Given a valid Dockerfile
// When the `rebuild` subcommand is run with a `--tag=` argument
// Then (A) the command fails
//     AND (B) the command STDOUT is empty
//     AND (C) the command STDERR contains an error message
fn prefix_tag_argument() {
    // (1)
    let test = test_setup::assert_apply(&Definition{
        name: "prefix_tag_argument",
        dockerfile_steps: "",
        fs: &hashmap!{},
    });
    let mut cmd = new_test_cmd(test.dir, &test.image_tagged_name);
    let arg = &("--tag=".to_owned() + &test.image_tagged_name);
    cmd.arg(arg);

    let cmd_result = cmd.assert();

    cmd_result
        // (A)
        .failure()
        // (B)
        .stdout("")
        // (C)
        .stderr(format!("unsupported argument: `{}`\n", arg));
}
