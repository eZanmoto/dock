// Copyright 2022-2024 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitStatus;
use std::str;
use std::string::FromUtf8Error;
use std::thread;
use std::time::Duration;

use crate::assert_run;
use crate::docker;
use crate::pty::Pty;
use crate::test_setup;
use crate::test_setup::Definition;
use crate::test_setup::References;

use crate::assert_cmd::assert::Assert;
use crate::assert_cmd::Command as AssertCommand;
use crate::nix::sys::time::TimeVal;
use crate::nix::sys::time::TimeValLike;
use crate::predicates::prelude::predicate::str as predicate_str;
use crate::predicates::str::RegexPredicate;

#[test]
// Given (1) the dock file defines an empty environment called `<env>`
//     AND (2) the target image defined by `<env>` doesn't exist
// When `run-in <env> true` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT is empty
//     AND (D) the target image exists
fn run_in_creates_image_if_none() {
    let test_name = "run_creates_image_if_none";
    // (1)
    let test = test_setup::assert_apply_with_empty_dock_yaml(&Definition{
        name: test_name,
        dockerfile_steps: "",
        fs: &hashmap!{},
    });
    // (2)
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result = run_test_cmd(&test.dir, &[test_name, "true"]);

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

pub fn run_test_cmd(root_test_dir: &str, args: &[&str]) -> Assert {
    let mut cmd = AssertCommand::cargo_bin(env!("CARGO_PKG_NAME"))
        .expect("couldn't create command for package binary");
    cmd.args(vec!["run-in"]);
    cmd.args(args);
    cmd.current_dir(root_test_dir);
    cmd.env_clear();
    // We set `HOME` because if unset then Docker BuildKit will create a
    // `.docker` directory in the working directory during builds.
    cmd.env("HOME", env!("HOME"));

    if let Ok(v) = env::var(DOCK_HOSTPATHS_VAR_NAME) {
        cmd.env(DOCK_HOSTPATHS_VAR_NAME, v);
    }

    cmd.assert()
}

const DOCK_HOSTPATHS_VAR_NAME: &str = "DOCK_HOSTPATHS";

#[test]
// Given (1) the dock file defines an empty environment called `<env>`
//     AND (2) `<env>`'s Dockerfile creates a test file
//     AND (3) the target image defined by `<env>` doesn't exist
// When `run-in <env> cat test.txt` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of the test file
//     AND (D) the target image exists
fn run_in_uses_correct_image() {
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

    let cmd_result = run_test_cmd(&test.dir, &[test_name, "cat", "test.txt"]);

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
// When `run-in <env> sh -c 'exit 2'` is run
// Then (A) the command returns an exit code of 2
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT is empty
//     AND (D) the target image exists
//     AND (E) no containers exist for the target image
fn run_in_returns_correct_exit_code() {
    let test_name = "run_returns_correct_exit_code";
    // (1)
    let test = test_setup::assert_apply_with_empty_dock_yaml(&Definition{
        name: test_name,
        dockerfile_steps: "",
        fs: &hashmap!{},
    });
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result =
        run_test_cmd(&test.dir, &[test_name, "sh", "-c", "exit 2"]);

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
// When `run-in <env> cat test.txt` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of `test.txt`
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

    let cmd_result = run_test_cmd(&test.dir, &[test_name, "cat", "test.txt"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(test_name);
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` uses the directory `dir` as the context
//     AND (3) `dir` contains `test.txt`
//     AND (4) `<env>`'s Dockerfile copies `test.txt`
// When `run-in <env> cat test.txt` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of `test.txt`
fn build_with_nested_directory_as_context() {
    let test_name = "build_with_nested_directory_as_context";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            context: ./dir
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

    let cmd_result = run_test_cmd(&test.dir, &[test_name, "cat", "test.txt"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(test_name);
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` uses the current directory as the context
//     AND (3) the current directory contains `test.txt`
//     AND (4) `<env>`'s Dockerfile copies `test.txt`
// When `run-in <env> cat test.txt` is run in a sub-directory
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of `test.txt`
fn run_in_from_subdir() {
    let test_name = "run_from_subdir";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            context: .
        "},
        &Definition{
            name: test_name,
            fs: &hashmap!{
                "dir/dummy.txt" => "",
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

    let cmd_result = run_test_cmd_from_subdir(
        &test.dir,
        Path::new("dir"),
        &[test_name, "cat", "test.txt"],
    );

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(test_name);
}

pub fn run_test_cmd_from_subdir(
    root_test_dir: &str,
    subdir: &Path,
    args: &[&str],
) -> Assert {
    let mut cmd = AssertCommand::cargo_bin(env!("CARGO_PKG_NAME"))
        .expect("couldn't create command for package binary");
    cmd.args(vec!["run-in"]);
    cmd.args(args);

    let mut p = Path::new(&root_test_dir).to_path_buf();
    p.push(subdir);
    cmd.current_dir(p);

    cmd.env_clear();
    // We set `HOME` because if unset then Docker BuildKit will create a
    // `.docker` directory in the working directory during builds.
    cmd.env("HOME", env!("HOME"));

    cmd.assert()
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) the container runs as root by default
//     AND (3) the local user doesn't have user ID 0
// When `run-in <env> id -u` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains `0`
fn run_in_without_local_user() {
    let test_name = "run_without_local_user";
    // (1)
    let test = test_setup::assert_apply_with_empty_dock_yaml(&Definition{
        name: test_name,
        // (2)
        dockerfile_steps: indoc!{"
            USER root
        "},
        fs: &hashmap!{},
    });
    docker::assert_remove_image(&test.image_tagged_name);
    let user_id = assert_run::assert_run_stdout("id", &["--user"]);
    // (3)
    assert_ne!(user_id.trim_end(), "0");

    let cmd_result = run_test_cmd(&test.dir, &[test_name, "id", "-u"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout("0\n");
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) the container runs as root by default
//     AND (3) the local user doesn't have group ID 0
// When `run-in <env> id -u` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains `0`
fn run_in_without_local_group() {
    let test_name = "run_without_local_group";
    // (1)
    let test = test_setup::assert_apply_with_empty_dock_yaml(&Definition{
        name: test_name,
        // (2)
        dockerfile_steps: indoc!{"
            USER root
        "},
        fs: &hashmap!{},
    });
    docker::assert_remove_image(&test.image_tagged_name);
    let user_id = assert_run::assert_run_stdout("id", &["--group"]);
    // (3)
    assert_ne!(user_id.trim_end(), "0");

    let cmd_result = run_test_cmd(&test.dir, &[test_name, "id", "-g"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout("0\n");
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` enables `user`
//     AND (3) the container runs as root by default
//     AND (4) the local user has user ID `<user_id>`
// When `run-in <env> id -u` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains `<user_id>`
fn run_in_with_local_user() {
    let test_name = "run_with_local_user";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            mount_local:
            - user
        "},
        &Definition{
            name: test_name,
            fs: &hashmap!{},
            // (3)
            dockerfile_steps: indoc!{"
                USER root
            "},
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);
    // (4)
    let user_id = assert_run::assert_run_stdout("id", &["--user"]);

    let cmd_result = run_test_cmd(&test.dir, &[test_name, "id", "-u"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(user_id);
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` enables `user` and `group`
//     AND (3) the container runs as root by default
//     AND (4) the local user has group ID `<group_id>`
// When `run-in <env> id -g` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains `<group_id>`
fn run_in_with_local_group() {
    let test_name = "run_with_local_group";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            mount_local:
            - user
            - group
        "},
        &Definition{
            name: test_name,
            fs: &hashmap!{},
            // (3)
            dockerfile_steps: indoc!{"
                USER root
            "},
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);
    // (4)
    let user_id = assert_run::assert_run_stdout("id", &["--group"]);

    let cmd_result = run_test_cmd(&test.dir, &[test_name, "id", "-g"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(user_id);
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` adds an `--env=X=a` and `--env=Y=b` argument
// When `run-in <env> sh -c 'echo $X $Y'` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains "a b"
fn run_in_with_env_var() {
    let test_name = "run_with_env_var";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            run_args:
            - --env=X=a
            - --env=Y=b
        "},
        &Definition{
            name: test_name,
            fs: &hashmap!{},
            dockerfile_steps: "",
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result =
        run_test_cmd(&test.dir, &[test_name, "sh", "-c", "echo $X $Y"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout("a b\n");
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` adds a `--user=1234` argument
// When `run-in <env> id -u` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains "1234"
fn run_in_with_specific_user() {
    let test_name = "run_with_specific_user";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            run_args:
            - --user=1234
        "},
        &Definition{
            name: test_name,
            fs: &hashmap!{},
            dockerfile_steps: "",
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result = run_test_cmd(&test.dir, &[test_name, "id", "-u"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout("1234\n");
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>`'s Dockerfile installs a Docker client
//     AND (3) `<env>` enables `nested_docker`
// When `run-in <env> docker version` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the target image exists
fn run_in_with_nested_docker() {
    let test_name = "run_with_nested_docker";
    // (1)
    let test = assert_apply_with_dockerfile(&TestDefinition{
        name: test_name,
        // (2)
        dockerfile: indoc!{"
            FROM docker:19.03.8
        "},
        // (3)
        env_defn: indoc!{"
            mount_local:
            - docker
        "},
    });
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result =
        run_test_cmd(&test.dir, &[test_name, "docker", "version"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("");
    // (C)
    docker::assert_image_exists(&test.image_tagged_name);
}

pub fn assert_apply_with_dockerfile(defn: &TestDefinition) -> References {
    // NOTE There is a lot of duplication between this function and
    // `tests::test_setup::assert_apply_with_dock_yaml`; this should ideally be
    // abstracted if an appropriate abstraction presents itself.

    let test_dir = test_setup::assert_create_root_dir(defn.name);

    let dock_file =
        test_setup::render_dock_file("0.1", defn.name, defn.env_defn);
    let dockerfile_name: &str = &format!("{}.Dockerfile", defn.name);

    let fs_state = &hashmap!{
        dockerfile_name => defn.dockerfile,
        "dock.yaml" => &dock_file,
    };
    test_setup::assert_write_fs_state(&test_dir, fs_state);

    let image_tagged_name =
        format!("{}.{}:latest", test_setup::IMAGE_NAME_ROOT, defn.name);

    References{
        dir: test_dir,
        image_tagged_name,
        cache_volume_prefix: test_setup::cache_volume_prefix(defn.name),
    }
}

pub struct TestDefinition<'a> {
    pub name: &'a str,
    pub dockerfile: &'a str,
    pub env_defn: &'a str,
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` mounts the project directory to `/host`
//     AND (3) the current directory contains `test.txt`
// When `run-in <env> cat /host/test.txt` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of `test.txt`
fn mount_proj_dir() {
    let test_name = "mount_proj_dir";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            mounts:
              .: /host
        "},
        &Definition{
            name: test_name,
            fs: &hashmap!{
                // (3)
                "test.txt" => test_name,
            },
            dockerfile_steps: "",
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result =
        run_test_cmd(&test.dir, &[test_name, "cat", "/host/test.txt"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(test_name.to_owned());
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` mounts `a/b` to `/host`
//     AND (3) the subdirectory `a/b/c/d` contains `test.txt`
// When `run-in <env> cat /host/c/d/test.txt` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of `test.txt`
fn mount_sub_dir() {
    let test_name = "mount_sub_dir";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            mounts:
              ./a/b: /host
        "},
        &Definition{
            name: test_name,
            fs: &hashmap!{
                // (3)
                "a/b/c/d/test.txt" => test_name,
            },
            dockerfile_steps: "",
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result =
        run_test_cmd(&test.dir, &[test_name, "cat", "/host/c/d/test.txt"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(test_name.to_owned());
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` defines the workdir as `/a/b
//     AND (3) the Dockerfile creates `test.txt` in `/a/b/c/d`
// When `run-in <env> cat c/d/test.txt` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of `test.txt`
fn workdir() {
    let test_name = "workdir";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            workdir: /a/b
        "},
        &Definition{
            name: test_name,
            // (3)
            dockerfile_steps: &formatdoc!{
                "
                    RUN mkdir --parents /a/b/c/d
                    RUN echo '{test_name}' > /a/b/c/d/test.txt
                ",
                test_name = test_name,
            },
            fs: &hashmap!{},
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result =
        run_test_cmd(&test.dir, &[test_name, "cat", "c/d/test.txt"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(test_name.to_owned() + "\n");
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` defines an environment variable `TEST`
// When `run-in <env> sh -c 'echo $TEST'` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of `TEST`
fn env_var() {
    let test_name = "env_var";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            env:
                TEST: contents
        "},
        &Definition{
            name: test_name,
            dockerfile_steps: "",
            fs: &hashmap!{},
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result =
        run_test_cmd(&test.dir, &[test_name, "sh", "-c", "echo $TEST"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout("contents\n");
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` defines `workdir` as `/a/b`
//     AND (3) `<env>` enables `project_dir`
//     AND (4) the current directory contains `test.txt`
// When `run-in <env> cat /a/b/test.txt` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of `test.txt`
fn project_dir() {
    let test_name = "project_dir";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2) (3)
        indoc!{"
            workdir: '/a/b'
            mount_local:
            - project_dir
        "},
        &Definition{
            name: test_name,
            dockerfile_steps: "",
            // (4)
            fs: &hashmap!{"test.txt" => test_name},
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result =
        run_test_cmd(&test.dir, &[test_name, "cat", "/a/b/test.txt"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(test_name.to_owned());
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` defines a cache volume called `test` at `/a/b`
//     AND (3) the Dockerfile used by `<env>` puts a test file in `/`
//     AND (4) the cache volume for `test` doesn't exist
//     AND (5) `run-in <env> cp /test.txt /a/b` was run
// When `run-in <env> cat /a/b/test.txt` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of `test.txt`
fn cache_volume() {
    let test_name = "cache_volume";
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
    docker::assert_remove_volume(&test.cache_volume_name("test"));
    // (5)
    run_test_cmd(&test.dir, &[test_name, "cp", "/test.txt", "/a/b"])
        .success();

    let cmd_result =
        run_test_cmd(&test.dir, &[test_name, "cat", "/a/b/test.txt"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(test_name.to_owned());
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` defines a cache volume called `test` at `/a/b`
//     AND (3) the Dockerfile used by `<env>` sets the user to non-root
//     AND (4) the cache volume for `test` doesn't exist
// When `run-in <env> touch /a/b/test.txt` is run
// Then (A) the command is successful
fn cache_volume_has_open_permission() {
    let test_name = "cache_volume_has_open_permission";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            cache_volumes:
              test: '/a/b'
        "},
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
    docker::assert_remove_volume(&test.cache_volume_name("test"));

    let cmd_result =
        run_test_cmd(&test.dir, &[test_name, "touch", "/a/b/test.txt"]);

    // (A)
    cmd_result.code(0);
}

// TODO Add test that creating a file in a non-cache volume fails.

#[test]
// Given (1) the dock file defines an empty environment called `<env>`
// When `run-in --debug <env> echo hello` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT includes debugging output
fn debug_flag() {
    let test_name = "debug_flag";
    // (1)
    let test = test_setup::assert_apply_with_empty_dock_yaml(&Definition{
        name: test_name,
        dockerfile_steps: "",
        fs: &hashmap!{},
    });
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result =
        run_test_cmd(&test.dir, &["-D", test_name, "echo", "hello"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(predicate_match(r"\[\$\] docker build "))
        .stdout(predicate_match(r"\[!\] #[0-9]+ building with .default. "))
        .stdout(predicate_match(r"\[!\] #[0-9]+ exporting to image"))
        .stdout(predicate_match(r"\[!\] #[0-9]+ naming to"))
        .stdout(predicate_match(r"\[\$\] docker run .*"))
        .stdout(predicate_match(r"hello"));
    docker::assert_image_exists(&test.image_tagged_name);
}

pub fn predicate_match(s: &str) -> RegexPredicate {
    predicate_str::is_match(s)
        .unwrap_or_else(|e| panic!(
            "couldn't generate a pattern match for '{s}': {e}",
        ))
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` adds a `--build-arg` build argument for `TEST_VALUE`
//     AND (3) `<env>`'s Dockerfile saves `TEST_VALUE` in `/test.txt`
// When `run-in <env> cat /test.txt` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the value of `TEST_VALUE`
fn run_in_with_build_args() {
    let test_name = "run_with_build_args";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            build_args:
            - --build-arg=TEST_VALUE=test-value
        "},
        &Definition{
            name: test_name,
            fs: &hashmap!{},
            // (3)
            dockerfile_steps: "
                ARG TEST_VALUE
                RUN echo \"$TEST_VALUE\" > /test.txt
            ",
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result = run_test_cmd(&test.dir, &[test_name, "cat", "/test.txt"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout("test-value\n");
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` has a `<script>` that checks if all streams are TTYs
// When `run-in --tty <env> sh <script>` is run with a PTY
// Then (A) the command returns 0
fn run_in_in_pty_with_tty_is_tty() {
    let test_name = "run_in_pty_with_tty_is_tty";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        indoc!{"
            context: .
        "},
        &Definition{
            name: test_name,
            dockerfile_steps: indoc!{"
                COPY check_ttys.sh /
            "},
            fs: &hashmap!{
                // (2)
                "check_ttys.sh" => indoc!{"
                    exit_code=0
                    die() {
                        echo \"$1\" >&2
                        exit_code=1
                    }

                    test -t 0 || die 'stdin is not a TTY'
                    test -t 1 || die 'stdout is not a TTY'
                    test -t 2 || die 'stderr is not a TTY'

                    exit $exit_code
                "},
            },
        },
    );
    let args = &["--tty", test_name, "sh", "/check_ttys.sh"];

    let cmd_result = run_test_cmd_with_pty(&test.dir, args);

    // (A)
    cmd_result.code(0);
}

fn run_test_cmd_with_pty(root_test_dir: &str, args: &[&str]) -> PtyResult {
    let prog = test_setup::test_bin();

    let mut run_args = vec!["run-in"];
    run_args.extend(args);

    let mut pty =
        unsafe { Pty::new(prog.as_os_str(), &run_args, root_test_dir) };

    // FIXME Reading to the end of the stream before waiting for the child to
    // exit could result in reads getting blocked; this should ideally be
    // refactored to use Tokio for more reliable test failures.

    let raw_output = read_to_end(&mut pty, Some(TimeVal::seconds(10)));

    let exit_status;
    let mut i = 0;
    loop {
        let maybe_exit_status = pty.try_wait()
            .expect("couldn't try to wait for child");

        if let Some(s) = maybe_exit_status {
            exit_status = s;
            break;
        } else if i > 10 {
            let output = str::from_utf8(&raw_output)
                .expect("output wasn't valid UTF-8");
            panic!("process didn't exit within timeout: {output}");
        }

        thread::sleep(Duration::from_secs(1));

        i += 1;
    }

    PtyResult{exit_status, output: raw_output}
}

// TODO Consider making `timeout` an attribute of `Pty` so that `Pty` can then
// implement `Read`, and so the default implementation of `read_to_end` can be
// used.
fn read_to_end(pty: &mut Pty, timeout: Option<TimeVal>) -> Vec<u8> {
    let mut buf = vec![];
    let mut buf_used = 0;
    loop {
        if buf.len() - buf_used < BUF_MIN_SPACE {
            buf.resize(buf.len() + BUF_MIN_SPACE, 0);
        }

        let n = pty.read(&mut buf[buf_used..], timeout)
            .expect("couldn't read from PTY")
            .expect("timeout occurred while reading from PTY");

        buf_used += n;

        if n == 0 {
            break;
        }
    }
    buf.resize(buf_used, 0);

    buf
}

const BUF_MIN_SPACE: usize = 0x100;

struct PtyResult {
    exit_status: ExitStatus,
    output: Vec<u8>,
}

impl PtyResult {
    fn code(self, exp: i32) -> PtyResult {
        if let Some(code) = self.exit_status.code() {
            if code != exp {
                self.fail(&format!("unexpected exit code: expected {exp}"));
            }
        } else {
            let status = self.exit_status;
            self.fail(&format!("couldn't extract exit code: {status}"));
        }

        self
    }

    fn fail(&self, msg: &str) -> ! {
        let output = str::from_utf8(&self.output)
            .expect("invalid UTF-8");

        let prefixed_output = prefix_lines("[>] ", output)
            .expect("invalid UTF-8");

        let summary = format!(
            "{}\n---\n{}\noutput:\n{}",
            msg,
            self.exit_status,
            prefixed_output,
        );

        let indented_summary = prefix_lines("    ", &summary)
            .expect("invalid UTF-8");

        panic!("\n{indented_summary}");
    }
}

fn prefix_lines(prefix: &str, buf: &str) -> Result<String, FromUtf8Error> {
    let s = Prefixer::new(prefix.as_bytes()).prefix(buf.as_bytes());

    String::from_utf8(s)
}

// TODO Duplicated from `src/cmd_loggers.rs`.
pub struct Prefixer<'a> {
    prefix: &'a [u8],
    due_prefix: bool,
}

// TODO Duplicated from `src/cmd_loggers.rs`.
impl<'a> Prefixer<'a> {
    pub fn new(prefix: &'a [u8]) -> Self {
        Prefixer{prefix, due_prefix: true}
    }

    // TODO This is likely to inefficient due to the creation of new values
    // instead of borrowing.
    pub fn prefix(&mut self, buf: &[u8]) -> Vec<u8> {
        if buf.is_empty() {
            return vec![];
        }

        let mut prefixed_buf = vec![];

        let mut first = true;
        for bs in buf.split_inclusive(|b| is_newline(*b)) {
            if !first || self.due_prefix {
                prefixed_buf.extend(self.prefix);
            }
            first = false;

            prefixed_buf.extend(bs);
        }

        let last = &buf[buf.len() - 1];
        self.due_prefix = is_newline(*last);

        prefixed_buf
    }
}

// TODO Duplicated from `src/cmd_loggers.rs`.
fn is_newline(b: u8) -> bool {
    b == NEWLINE
}

// TODO Duplicated from `src/cmd_loggers.rs`.
const NEWLINE: u8 = 0x0a;

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` has a `<script>` that checks if all streams are TTYs
// When `run-in --tty <env> sh <script>` is run with a PTY
// Then (A) the command returns 0
fn run_in_in_pty_without_tty_is_not_tty() {
    let test_name = "run_in_pty_without_tty_is_not_tty";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        indoc!{"
            context: .
        "},
        &Definition{
            name: test_name,
            dockerfile_steps: indoc!{"
                COPY check_ttys.sh /
            "},
            fs: &hashmap!{
                // (2)
                "check_ttys.sh" => indoc!{"
                    exit_code=0
                    die() {
                        echo \"$1\" >&2
                        exit_code=1
                    }

                    test -t 0 || die 'stdin is not a TTY'
                    test -t 1 || die 'stdout is not a TTY'
                    test -t 2 || die 'stderr is not a TTY'

                    exit $exit_code
                "},
            },
        },
    );

    let cmd_result =
        run_test_cmd_with_pty(&test.dir, &[test_name, "sh", "/check_ttys.sh"]);

    // (A)
    cmd_result.code(1);
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` has a `<script>` that checks if all streams are TTYs
// When `run-in --tty <env> sh <script>` is run
// Then (A) the command returns 0
fn run_in_with_tty_is_tty() {
    let test_name = "run_with_tty_is_tty";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        indoc!{"
            context: .
        "},
        &Definition{
            name: test_name,
            dockerfile_steps: indoc!{"
                COPY check_ttys.sh /
            "},
            fs: &hashmap!{
                // (2)
                "check_ttys.sh" => indoc!{"
                    exit_code=0
                    die() {
                        echo \"$1\" >&2
                        exit_code=1
                    }

                    test -t 0 || die 'stdin is not a TTY'
                    test -t 1 || die 'stdout is not a TTY'
                    test -t 2 || die 'stderr is not a TTY'

                    exit $exit_code
                "},
            },
        },
    );

    let cmd_result =
        run_test_cmd(&test.dir, &["--tty", test_name, "sh", "/check_ttys.sh"]);

    // (A)
    cmd_result.code(0);
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) a test file contains `"a"`
//     AND (3) the target image for `<env>` is built with the test file
//     AND (4) the test file is updated to contain `"b"`
// When `run-in <env> cat /test.txt` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains `"b"`
fn run_in_without_skip_rebuild_rebuilds() {
    let test_name = "run_without_skip_rebuild_rebuilds";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        indoc!{"
            context: .
        "},
        &Definition{
            name: test_name,
            fs: &hashmap!{
                // (2)
                "test.txt" => "a",
            },
            dockerfile_steps: indoc!{"
                COPY test.txt /
            "},
        },
    );
    // (3)
    run_test_cmd(&test.dir, &[test_name, "true"]).code(0);
    let test_file_path = format!("{}/test.txt", test.dir);
    // (4)
    fs::write(test_file_path, "b")
        .expect("couldn't write Dockerfile");

    let cmd_result = run_test_cmd(&test.dir, &[test_name, "cat", "/test.txt"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout("b");
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) a test file contains `"a"`
//     AND (3) the target image for `<env>` is built with the test file
//     AND (4) the test file is updated to contain `"b"`
// When `run-in --skip-rebuild <env> cat /test.txt` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains `"a"`
fn run_in_with_skip_rebuild_doesnt_rebuild() {
    let test_name = "run_with_skip_rebuild_doesnt_rebuild";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        indoc!{"
            context: .
        "},
        &Definition{
            name: test_name,
            fs: &hashmap!{
                // (2)
                "test.txt" => "a",
            },
            dockerfile_steps: indoc!{"
                COPY test.txt /
            "},
        },
    );
    // (3)
    run_test_cmd(&test.dir, &[test_name, "true"]).code(0);
    let test_file_path = format!("{}/test.txt", test.dir);
    // (4)
    fs::write(test_file_path, "b")
        .expect("couldn't write Dockerfile");
    let args = &["--skip-rebuild", test_name, "cat", "/test.txt"];

    let cmd_result = run_test_cmd(&test.dir, args);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout("a");
}

#[test]
// Given (1) the dock file defines an empty environment called `<env>`
// When `run-in <env>-env: echo hi` is run
// Then (A) the command returns an exit code of 0
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains 'hi'
fn run_in_with_extended_env_flag() {
    let test_name = "run_in_with_extended_env_flag";
    // (1)
    let test = test_setup::assert_apply_with_empty_dock_yaml(&Definition{
        name: test_name,
        dockerfile_steps: "",
        fs: &hashmap!{},
    });
    let env_arg = &format!("{test_name}-env:");

    let cmd_result = run_test_cmd(&test.dir, &[env_arg, "echo", "hi"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout("hi\n");
}
