// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::str;

use crate::docker;
use crate::test_setup;
use crate::test_setup::Definition;

use crate::assert_cmd::assert::Assert;
use crate::assert_cmd::Command as AssertCommand;

#[test]
// Given (1) the dock file defines an empty environment called `<env>`
//     AND (2) the target image defined by `<env>` doesn't exist
// When `run <env> true` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT is empty
//     AND (D) the target image exists
fn run_creates_image_if_none() {
    let test_name = "run_creates_image_if_none";
    // (1)
    let test = test_setup::assert_apply_with_empty_dock_yaml(&Definition{
        name: test_name,
        dockerfile_steps: "",
        fs: &hashmap!{},
    });
    // (2)
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result = run_test_cmd(test.dir, &[test_name, "true"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout("");
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
}

pub fn run_test_cmd(root_test_dir: String, args: &[&str]) -> Assert {
    let mut cmd = AssertCommand::cargo_bin(env!("CARGO_PKG_NAME"))
        .expect("couldn't create command for package binary");
    cmd.args(vec!["run"]);
    cmd.args(args);
    cmd.current_dir(root_test_dir);
    cmd.env_clear();

    cmd.assert()
}

#[test]
// Given (1) the dock file defines an empty environment called `<env>`
//     AND (2) `<env>`'s Dockerfile creates a test file
//     AND (3) the target image defined by `<env>` doesn't exist
// When `run <env> cat test.txt` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of the test file
//     AND (D) the target image exists
fn run_uses_correct_image() {
    let test_name = "run_uses_correct_image";
    // (1)
    let test = test_setup::assert_apply_with_empty_dock_yaml(&Definition{
        name: test_name,
        // (2)
        dockerfile_steps: &formatdoc!{
            "
                RUN echo '{test_name}' > test.txt
            ",
            test_name = test_name,
        },
        fs: &hashmap!{},
    });
    // (3)
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result = run_test_cmd(test.dir, &[test_name, "cat", "test.txt"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(test_name.to_owned() + "\n");
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
}

#[test]
// Given (1) the dock file defines an empty environment called `<env>`
// When `run <env> sh -c 'exit 2'` is run
// Then (A) the command returns an exit code of 2
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT is empty
//     AND (D) the target image exists
//     AND (E) no containers exist for the target image
fn run_returns_correct_exit_code() {
    let test_name = "run_returns_correct_exit_code";
    // (1)
    let test = test_setup::assert_apply_with_empty_dock_yaml(&Definition{
        name: test_name,
        dockerfile_steps: "",
        fs: &hashmap!{},
    });
    // (2)
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result =
        run_test_cmd(test.dir, &[test_name, "sh", "-c", "exit 2"]);

    cmd_result
        // (A)
        .code(2)
        // (B)
        .stderr("")
        // (C)
        .stdout("");
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
    // (E)
    docker::assert_no_containers_from_image(&test.image_tagged_name);
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` uses the current directory as the context
//     AND (3) the current directory contains `test.txt`
//     AND (4) `<env>`'s Dockerfile copies `test.txt`
// When `run <env> cat test.txt` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of `test.txt`
//     AND (D) the target image exists
fn build_with_project_directory_as_context() {
    let test_name = "build_with_project_directory_as_context";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            context: .
        "},
        &Definition{
            name: test_name,
            fs: &hashmap!{
                // (3)
                "test.txt" => test_name,
            },
            // (4)
            dockerfile_steps: indoc!{"
                COPY test.txt /
            "},
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result = run_test_cmd(test.dir, &[test_name, "cat", "test.txt"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(test_name);
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` uses the directory `dir` as the context
//     AND (3) `dir` contains `test.txt`
//     AND (4) `<env>`'s Dockerfile copies `test.txt`
// When `run <env> cat test.txt` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of `test.txt`
//     AND (D) the target image exists
fn build_with_nested_directory_as_context() {
    let test_name = "build_with_nested_directory_as_context";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            context: dir
        "},
        &Definition{
            name: test_name,
            fs: &hashmap!{
                // (3)
                "dir/test.txt" => test_name,
            },
            // (4)
            dockerfile_steps: indoc!{"
                COPY test.txt /
            "},
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result = run_test_cmd(test.dir, &[test_name, "cat", "test.txt"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(test_name);
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
}
