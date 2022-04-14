// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::env;
use std::path::Path;
use std::str;

use crate::assert_run;
use crate::docker;
use crate::test_setup;
use crate::test_setup::Definition;
use crate::test_setup::References;

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

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` uses the current directory as the context
//     AND (3) the current directory contains `test.txt`
//     AND (4) `<env>`'s Dockerfile copies `test.txt`
// When `run <env> cat test.txt` is run in a sub-directory
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of `test.txt`
//     AND (D) the target image exists
fn run_from_subdir() {
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
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
}

pub fn run_test_cmd_from_subdir(
    root_test_dir: &str,
    subdir: &Path,
    args: &[&str],
) -> Assert {
    let mut cmd = AssertCommand::cargo_bin(env!("CARGO_PKG_NAME"))
        .expect("couldn't create command for package binary");
    cmd.args(vec!["run"]);
    cmd.args(args);

    let mut p = Path::new(&root_test_dir).to_path_buf();
    p.push(subdir);
    cmd.current_dir(p);

    cmd.env_clear();

    cmd.assert()
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) the container runs as root by default
//     AND (3) the local user doesn't have user ID 0
// When `run <env> id -u` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains `0`
//     AND (D) the target image exists
fn run_without_local_user() {
    let test_name = "run_without_local_user";
    // (1)
    let test = test_setup::assert_apply_with_empty_dock_yaml(&Definition{
        name: test_name,
        // (2)
        dockerfile_steps: &indoc!{"
            USER root
        "},
        fs: &hashmap!{},
    });
    docker::assert_remove_image(&test.image_tagged_name);
    let user_id = assert_run::assert_run_stdout("id", &["--user"]);
    // (3)
    assert_ne!(user_id.trim_end(), "0");

    let cmd_result = run_test_cmd(test.dir, &[test_name, "id", "-u"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout("0\n");
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) the container runs as root by default
//     AND (3) the local user doesn't have group ID 0
// When `run <env> id -u` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains `0`
//     AND (D) the target image exists
fn run_without_local_group() {
    let test_name = "run_without_local_group";
    // (1)
    let test = test_setup::assert_apply_with_empty_dock_yaml(&Definition{
        name: test_name,
        // (2)
        dockerfile_steps: &indoc!{"
            USER root
        "},
        fs: &hashmap!{},
    });
    docker::assert_remove_image(&test.image_tagged_name);
    let user_id = assert_run::assert_run_stdout("id", &["--group"]);
    // (3)
    assert_ne!(user_id.trim_end(), "0");

    let cmd_result = run_test_cmd(test.dir, &[test_name, "id", "-g"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout("0\n");
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` enables `local_user_group`
//     AND (3) the container runs as root by default
//     AND (4) the local user has user ID `<user_id>`
// When `run <env> id -u` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains `<user_id>`
//     AND (D) the target image exists
fn run_with_local_user() {
    let test_name = "run_with_local_user";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            enabled:
            - local_user_group
        "},
        &Definition{
            name: test_name,
            fs: &hashmap!{},
            // (3)
            dockerfile_steps: &indoc!{"
                USER root
            "},
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);
    // (4)
    let user_id = assert_run::assert_run_stdout("id", &["--user"]);

    let cmd_result = run_test_cmd(test.dir, &[test_name, "id", "-u"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(user_id);
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` enables `local_user_group`
//     AND (3) the container runs as root by default
//     AND (4) the local user has group ID `<group_id>`
// When `run <env> id -g` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains `<group_id>`
//     AND (D) the target image exists
fn run_with_local_group() {
    let test_name = "run_with_local_group";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            enabled:
            - local_user_group
        "},
        &Definition{
            name: test_name,
            fs: &hashmap!{},
            // (3)
            dockerfile_steps: &indoc!{"
                USER root
            "},
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);
    // (4)
    let user_id = assert_run::assert_run_stdout("id", &["--group"]);

    let cmd_result = run_test_cmd(test.dir, &[test_name, "id", "-g"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(user_id);
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` adds an `--env=X=a` and `--env=Y=b` argument
// When `run <env> sh -c 'echo $X $Y'` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains "a b"
//     AND (D) the target image exists
fn run_with_env_var() {
    let test_name = "run_with_env_var";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            args:
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
    let args = &[test_name, "sh", "-c", "echo $X $Y"];

    let cmd_result = run_test_cmd(test.dir, args);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout("a b\n");
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>` adds a `--user=1234` argument
// When `run <env> id -u` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains "1234"
//     AND (D) the target image exists
fn run_with_specific_user() {
    let test_name = "run_with_specific_user";
    // (1)
    let test = test_setup::assert_apply_with_dock_yaml(
        // (2)
        indoc!{"
            args:
            - --user=1234
        "},
        &Definition{
            name: test_name,
            fs: &hashmap!{},
            dockerfile_steps: "",
        },
    );
    docker::assert_remove_image(&test.image_tagged_name);

    let cmd_result = run_test_cmd(test.dir, &[test_name, "id", "-u"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout("1234\n");
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
}

#[test]
// Given (1) the dock file defines an environment called `<env>`
//     AND (2) `<env>`'s Dockerfile installs a Docker client
//     AND (3) `<env>` enables `nested_docker`
// When `run <env> docker version` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the target image exists
fn run_with_nested_docker() {
    let test_name = "run_with_nested_docker";
    // (1)
    let test = assert_apply(&TestDefinition{
        name: test_name,
        // (2)
        dockerfile: indoc!{"
            FROM docker:19.03.8
        "},
        // (3)
        env_defn: indoc!{"
            enabled:
            - nested_docker
        "},
    });
    docker::assert_remove_image(&test.image_tagged_name);
    let args = &[test_name, "docker", "version"];

    let cmd_result = run_test_cmd(test.dir, args);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("");
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
}

pub fn assert_apply(defn: &TestDefinition) -> References {
    // NOTE There is a lot of duplication between this function and
    // `tests::test_setup::assert_apply_with_dock_yaml`; this should ideally be
    // abstracted if an appropriate abstraction presents itself.

    let test_dir = test_setup::assert_create_root_dir(defn.name);

    let indented_env_defn =
        defn.env_defn
            .lines()
            .collect::<Vec<&str>>()
            .join("\n    ");

    let dock_file: &str = &formatdoc!{
        "
            organisation: 'ezanmoto'
            project: 'dock.test'

            environments:
              {test_name}:
                {env_defn}
        ",
        test_name = defn.name,
        env_defn = indented_env_defn,
    };
    let dockerfile_name: &str = &format!("{}.Dockerfile", defn.name);

    let fs_state = &hashmap!{
        dockerfile_name => defn.dockerfile,
        "dock.yaml" => dock_file,
    };
    test_setup::assert_write_fs_state(&test_dir, &fs_state);

    let image_tagged_name =
        format!("{}.{}:latest", test_setup::IMAGE_NAME_ROOT, defn.name);

    References{
        dir: test_dir,
        image_tagged_name,
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
// When `run <env> cat /host/test.txt` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of `test.txt`
//     AND (D) the target image exists
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
    let args = &[test_name, "cat", "/host/test.txt"];

    let cmd_result = run_test_cmd(test.dir, args);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(test_name.to_owned());
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
}
