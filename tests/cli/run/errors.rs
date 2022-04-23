// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::str;

use crate::cli::rebuild::success as rebuild_success;
use crate::docker;
use crate::test_setup;
use crate::test_setup::Definition;
use super::success;
use super::success::TestDefinition;

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

    let cmd_result = success::run_test_cmd(&test.dir, &[test_name, "true"]);

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
    let stdout = rebuild_success::new_str_from_cmd_stdout(&cmd_result);
    // (C)
    rebuild_success::assert_docker_build_stdout(&stdout);
    // (D)
    docker::assert_image_doesnt_exist(&test.image_tagged_name);
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
        success::run_test_cmd(&test.dir, &[test_name, "cat", "/nonexistent"]);

    cmd_result
        // (A)
        .code(predicate::ne(0))
        // (B)
        // NOTE See "Command Error Messages" in `tests/cli/README.md` for
        // caveats on this error message.
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
            context: ./dir
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

    let cmd_result = success::run_test_cmd(&test.dir, &[test_name, "true"]);

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

    let cmd_result = success::run_test_cmd(&test.dir, &[test_name, "true"]);

    cmd_result
        // (A)
        .code(1)
        // (B)
        .stderr(predicate_str::ends_with(
            "The relative path didn't start with `.`\n",
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

    let cmd_result = success::run_test_cmd(&test.dir, &[test_name, "true"]);

    cmd_result
        // (A)
        .code(1)
        // (B)
        .stderr(predicate_str::ends_with(
            "The relative path didn't start with `.`\n",
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

    let cmd_result = success::run_test_cmd(&test.dir, &[test_name, "true"]);

    cmd_result
        // (A)
        .code(1)
        // (B)
        .stderr(predicate_str::ends_with(
            "The relative path didn't start with `.`\n",
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
//     AND (2) `<env>`'s Dockerfile installs a Docker client
//     AND (3) `<env>` doesn't enable `nested_docker`
// When `run <env> docker version` is run
// Then (A) the command returns 1
//     AND (B) the command STDERR contains "no such host"
//     AND (C) the target image exists
fn run_without_nested_docker() {
    let test_name = "run_without_nested_docker";
    // (1)
    let test = success::assert_apply(&TestDefinition{
        name: test_name,
        // (2)
        dockerfile: "FROM docker:19.03.8",
        // (3)
        env_defn: "{}",
    });
    docker::assert_remove_image(&test.image_tagged_name);
    let args = &[test_name, "docker", "version"];

    let cmd_result = success::run_test_cmd(&test.dir, args);

    cmd_result
        // (A)
        .code(1)
        // (B)
        .stderr(predicate_str::contains("no such host"));
    // (C)
    docker::assert_image_exists(&test.image_tagged_name);
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` enables `project_dir`
//     AND (3) `<env>` doesn't define `workdir`
// When `run <env> cat /a/b/test.txt` is run
// Then (A) the command returns an exit code of 1
//     AND (B) the command STDERR contains "`workdir` is required"
//     AND (C) the command STDOUT is empty
//     AND (D) the target image exists
fn project_dir_without_workdir() {
    let test_name = "project_dir_without_workdir";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2) (3)
        indoc!{"
            enabled:
            - project_dir
        "},
        &Definition{
            name: test_name,
            dockerfile_steps: "",
            fs: &hashmap!{},
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);
    let args = &[test_name, "cat", "/a/b/test.txt"];

    let cmd_result = success::run_test_cmd(&test.dir, args);

    cmd_result
        // (A)
        .code(1)
        // (B)
        .stderr(predicate_str::contains("`workdir` is required"))
        // (C)
        .stdout("");
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` defines a cache volume called `test` at `/a/b`
//     AND (3) the Dockerfile used by `<env>` puts a test file in `/`
//     AND (4) `run <env> cp /test.txt /a/b` was run
//     AND (5) then the cache volume for `test` was deleted
// When `run <env> cat /a/b/test.txt` is run
// Then (A) the command returns a non-zero exit code
//     AND (B) the command STDERR contains the error message from `cat`
//     AND (C) the command STDOUT is empty
//     AND (D) the target image exists
fn removing_cache_volume_deletes_files() {
    let test_name = "removing_cache_volume_deletes_files";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            context: .
            cache_volumes:
              test: '/a/b'
        "},
        &Definition{
            name: test_name,
            // (3)
            dockerfile_steps: indoc!{"
                USER 10000
                COPY test.txt /
            "},
            fs: &hashmap!{"test.txt" => test_name},
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);
    // (4)
    success::run_test_cmd(&test.dir, &[test_name, "cp", "/test.txt", "/a/b"])
        .success();
    // (5)
    docker::assert_remove_volume(&test.cache_volume_name("test"));
    let args = &[test_name, "cat", "/a/b/test.txt"];

    let cmd_result = success::run_test_cmd(&test.dir, args);

    cmd_result
        // (A)
        .code(predicate::ne(0))
        // (B)
        // NOTE See "Command Error Messages" in `tests/cli/README.md` for
        // caveats on this error message.
        .stderr("cat: can't open '/a/b/test.txt': No such file or directory\n")
        // (C)
        .stdout("");
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` defines a volume at `/a/b`
//     AND (3) the Dockerfile used by `<env>` sets the user to non-root
//     AND (4) the volume doesn't exist
// When `run <env> touch /a/b/test.txt` is run
// Then (A) the command returns a non-zero exit code
//     AND (B) the command STDERR contains the error message from `touch`
//     AND (C) the command STDOUT is empty
//     AND (D) the target image exists
fn manual_volume_has_root_permission() {
    let test_name = "manual_volume_has_root_permission";
    let vol_name = test_setup::cache_volume_prefix(test_name) + ".test";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        &formatdoc!{
            "
                args:
                - --mount=type=volume,src={vol_name},dst=/a/b
            ",
            vol_name = vol_name,
        },
        &Definition{
            name: test_name,
            // (3)
            dockerfile_steps: indoc!{"
                USER 10000
            "},
            fs: &hashmap!{},
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);
    // (4)
    docker::assert_remove_volume(&vol_name);
    let args = &[test_name, "touch", "/a/b/test.txt"];

    let cmd_result = success::run_test_cmd(&test.dir, args);

    cmd_result
        // (A)
        .code(predicate::ne(0))
        // (B)
        // NOTE See "Command Error Messages" in `tests/cli/README.md` for
        // caveats on this error message.
        .stderr("touch: /a/b/test.txt: Permission denied\n")
        // (C)
        .stdout("");
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
}
