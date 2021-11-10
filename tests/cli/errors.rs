// Copyright 2021 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use crate::test_setup;

use crate::assert_cmd::Command as AssertCommand;

#[test]
// Given (1) the Dockerfile contains the `false` command
// When the `rebuild` subcommand is run
// Then (A) the command is not successful
fn failing_dockerfile_returns_non_zero() {
    let test_name = "failing_dockerfile_returns_non_zero";
    let test = test_setup::create(
        test_name,
        &hashmap!{
            "Dockerfile" => indoc!{"
                FROM alpine:3.14.2
                RUN false
            "},
        },
    );
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

    cmd
}
