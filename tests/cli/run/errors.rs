// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::str;

use crate::docker;
use crate::docker::build::DockerBuild;
use crate::test_setup;
use crate::test_setup::Definition;
use super::success;

use crate::assert_cmd::assert::Assert;
use crate::predicates::prelude::predicate;

#[test]
// Given (1) the dock file defines an empty environment called `<env>`
//     AND (2) the Dockerfile used by `<env>` has a step that fails
// When `run <env> true` is run
// Then (A) the command returns an exit code of 1
//     AND (B) the command STDERR contains the `docker build` STDERR
//     AND (C) the command STDOUT contains the `docker build` STDOUT
//     AND (D) the target image doesn't exist
fn run_with_build_failure() {
    // (1)
    let test_name = "run_with_build_failure";
    let test = test_setup::assert_apply_with_dock_yaml(&Definition{
        name: test_name,
        dockerfile_steps: indoc!{"
            RUN exit 2
        "},
        fs: &hashmap!{},
    });
    // (2)
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result = success::run_test_cmd(test.dir, &[test_name, "true"]);

    // NOTE This error message depends on the specific message returned by the
    // Docker server. This message is correct when using
    // `Docker Engine - Community` version `19.03.12` as the Docker server.
    let exp = "The command '/bin/sh -c exit 2' returned a non-zero code: 2\n";
    let cmd_result =
        cmd_result
            // (A)
            .code(1)
            // (B)
            .stderr(exp);
    // (C)
    let stdout = new_str_from_cmd_stdout(&cmd_result);
    let img_name = &test.image_tagged_name;
    DockerBuild::assert_parse_from_stdout(&mut stdout.lines(), img_name);
    // (D)
    assert!(!docker::assert_get_local_image_tagged_names().contains(img_name));
}

fn new_str_from_cmd_stdout(cmd_result: &Assert) -> &str {
    let stdout_bytes = &cmd_result.get_output().stdout;

    str::from_utf8(&stdout_bytes)
        .expect("couldn't decode STDOUT")
}

#[test]
// Given (1) the dock file defines an empty environment called `<env>`
// When `run <env> cat /nonexistent` is run
// Then (A) the command returns a non-zero exit code
//     AND (B) the command STDERR contains the error message from `cat`
//     AND (C) the command STDOUT is empty
//     AND (D) the target image exists
//     AND (E) no containers exist for the target image
fn run_with_run_failure() {
    // (1)
    let test_name = "run_with_run_failure";
    let test = test_setup::assert_apply_with_dock_yaml(&Definition{
        name: test_name,
        dockerfile_steps: "",
        fs: &hashmap!{},
    });

    let cmd_result =
        success::run_test_cmd(test.dir, &[test_name, "cat", "/nonexistent"]);

    cmd_result
        // (A)
        .code(predicate::ne(0))
        // (B)
        // NOTE This error message depends on the specific implementation of
        // the `cat` program in the image. As such, if a different base image
        // is used, this expected message may need to change.
        .stderr("cat: can't open '/nonexistent': No such file or directory\n")
        // (C)
        .stdout("");
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
    // (E)
    docker::assert_no_containers_from_image(&test.image_tagged_name);
}
