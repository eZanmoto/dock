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
use crate::predicates::prelude::predicate::str as predicate_str;
use crate::predicates::str::RegexPredicate;

#[test]
// Given (1) the dock file defines an empty environment called `<env>`
//     AND (2) the Dockerfile used by `<env>` has a step that fails
// When `run <env> true` is run
// Then (A) the command returns an exit code of 1
//     AND (B) the command STDERR contains the `docker build` STDERR
//     AND (C) the command STDOUT contains the `docker build` STDOUT
//     AND (D) the target image doesn't exist
fn run_with_build_failure() {
    let test_name = "run_with_build_failure";
    // (1)
    let test = test_setup::assert_apply_with_empty_dock_yaml(&Definition{
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
    docker::assert_image_doesnt_exist(img_name);
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
    let test_name = "run_with_run_failure";
    // (1)
    let test = test_setup::assert_apply_with_empty_dock_yaml(&Definition{
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

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` uses the directory `dir` as the context
//     AND (3) `dir` doesn't contain `test.txt`
//     AND (4) `<env>`'s Dockerfile copies `test.txt`
// When `run <env> true` is run
// Then (A) the command returns an exit code of 1
//     AND (B) the command STDERR indicates that the copy failed
//     AND (C) the target image doesn't exist
//     AND (D) no containers exist for the target image
fn build_with_file_outside_context_directory() {
    let test_name = "build_with_file_outside_context_directory";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            context: dir
        "},
        &Definition{
            name: test_name,
            // (3)
            fs: &hashmap!{
                "dir/dummy.txt" => "",
                "test.txt" => test_name,
            },
            // (4)
            dockerfile_steps: indoc!{"
                COPY test.txt /
            "},
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result = success::run_test_cmd(test.dir, &[test_name, "true"]);

    cmd_result
        // (A)
        .code(1)
        // (B)
        .stderr(predicate_match(
            "COPY failed: .*/test.txt: no such file or directory",
        ));
    // (C)
    docker::assert_image_doesnt_exist(&test.image_tagged_name);
    // (D)
    docker::assert_no_containers_from_image(&test.image_tagged_name);
}

fn predicate_match(s: &str) -> RegexPredicate {
    predicate_str::is_match(s)
        .unwrap_or_else(|e| panic!(
            "couldn't generate a pattern match for '{}': {}",
            s,
            e,
        ))
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) the `<env>` context path starts with `..`
// When `run <env> true` is run
// Then (A) the command returns an exit code of 1
//     AND (B) the command STDERR indicates the invalid path
//     AND (B) the command STDOUT is empty
//     AND (D) the target image doesn't exist
//     AND (E) no containers exist for the target image
fn context_starts_with_path_traversal() {
    let test_name = "context_starts_with_path_traversal";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            context: ../dir
        "},
        &Definition{
            name: test_name,
            fs: &hashmap!{},
            dockerfile_steps: indoc!{""},
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result = success::run_test_cmd(test.dir, &[test_name, "true"]);

    cmd_result
        // (A)
        .code(1)
        // (B)
        .stderr(predicate_str::starts_with(
            "context path can't contain traversal",
        ))
        // (C)
        .stdout("");
    // (D)
    docker::assert_image_doesnt_exist(&test.image_tagged_name);
    // (E)
    docker::assert_no_containers_from_image(&test.image_tagged_name);
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) the `<env>` context path contains a `..` component
// When `run <env> true` is run
// Then (A) the command returns an exit code of 1
//     AND (B) the command STDERR indicates the invalid path
//     AND (B) the command STDOUT is empty
//     AND (D) the target image doesn't exist
//     AND (E) no containers exist for the target image
fn context_contains_path_traversal() {
    let test_name = "context_contains_path_traversal";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            context: dir/../dir
        "},
        &Definition{
            name: test_name,
            fs: &hashmap!{},
            dockerfile_steps: indoc!{""},
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result = success::run_test_cmd(test.dir, &[test_name, "true"]);

    cmd_result
        // (A)
        .code(1)
        // (B)
        .stderr(predicate_str::starts_with(
            "context path can't contain traversal",
        ))
        // (C)
        .stdout("");
    // (D)
    docker::assert_image_doesnt_exist(&test.image_tagged_name);
    // (E)
    docker::assert_no_containers_from_image(&test.image_tagged_name);
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) the `<env>` context path starts with `/`
// When `run <env> true` is run
// Then (A) the command returns an exit code of 1
//     AND (B) the command STDERR indicates the invalid path
//     AND (B) the command STDOUT is empty
//     AND (D) the target image doesn't exist
//     AND (E) no containers exist for the target image
fn context_contains_absolute_path() {
    let test_name = "context_contains_absolute_path";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            context: /dir
        "},
        &Definition{
            name: test_name,
            fs: &hashmap!{},
            dockerfile_steps: indoc!{""},
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result = success::run_test_cmd(test.dir, &[test_name, "true"]);

    cmd_result
        // (A)
        .code(1)
        // (B)
        .stderr(predicate_str::starts_with(
            "context path can't contain traversal",
        ))
        // (C)
        .stdout("");
    // (D)
    docker::assert_image_doesnt_exist(&test.image_tagged_name);
    // (E)
    docker::assert_no_containers_from_image(&test.image_tagged_name);
}
