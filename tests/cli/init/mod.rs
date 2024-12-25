// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::env;
use std::fs;
use std::str;

use crate::assert_run;
use crate::test_setup;

use crate::assert_cmd::assert::Assert;
use crate::assert_cmd::Command as AssertCommand;
use crate::predicates::prelude::predicate::str as predicate_str;

// TODO Test directory contents after `init`.

#[test]
// Given (1) a Git repository `<source>` containing `<templ>`
//     AND (2) `<templ>` contains a dock file defining `<env>`
//     AND (3) `<templ>` contains a Dockerfile named `<env>.Dockerfile`
//     AND (4) `<env>.Dockerfile` creates a test file `<test>`
//     AND (5) an empty test directory `<dir>` exists
// When `dock init --source <source> <templ>` is run in `<dir>`
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the name of the dock file
//     AND (D) the command STDOUT contains `<env>.Dockerfile`
fn init_outputs_created_files() {
    let test_name = "init_outputs_created_files";
    let root_test_dir = test_setup::assert_create_root_dir(test_name);
    // (1) (2) (3) (4)
    let test_source_dir = create_templates_dir(&root_test_dir, test_name);
    assert_init_git_repo(&test_source_dir);
    // (5)
    let test_dir = test_setup::assert_create_dir(root_test_dir, "dir");
    let source = format!("git:{test_source_dir}:master:.");

    let cmd_result =
        run_test_cmd(&test_dir, &["init", "--source", &source, "templ"]);

    let dockerfile_msg = format!("Created './{test_name}.Dockerfile'");
    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(predicate_str::contains("Created './dock.yaml'"))
        // (D)
        .stdout(predicate_str::contains(dockerfile_msg));
}

#[test]
// Given (1) a Git repository `<source>` containing `<templ>`
//     AND (2) `<templ>` contains a dock file defining `<env>`
//     AND (3) `<templ>` contains a Dockerfile named `<env>.Dockerfile`
//     AND (4) `<env>.Dockerfile` creates a test file `<test>`
//     AND (5) an empty test directory `<dir>` exists
//     AND (6) `dock init --source <source> <templ>` is run in `<dir>`
// When `dock run-in <env> cat <test>` is run in `<dir>`
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of `<test>`
fn init_creates_env() {
    let test_name = "init_creates_env";
    let root_test_dir = test_setup::assert_create_root_dir(test_name);
    // (1) (2) (3) (4)
    let test_source_dir = create_templates_dir(&root_test_dir, test_name);
    assert_init_git_repo(&test_source_dir);
    // (5)
    let test_dir = test_setup::assert_create_dir(root_test_dir, "dir");
    let source = format!("git:{test_source_dir}:master:.");
    // (6)
    assert_test_cmd(&test_dir, &["init", "--source", &source, "templ"]);

    let cmd_result =
        run_test_cmd(&test_dir, &["run-in", test_name, "cat", "/test.txt"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(format!("{test_name}\n"));
}

fn create_templates_dir(root_test_dir: &str, test_name: &str) -> String {
    let test_dock_yaml = formatdoc!{
        "
            schema_version: '0.1'
            organisation: org
            project: proj
            default_shell_env: {test_name}

            environments:
              {test_name}:
                mount_local:
                - user
        ",
        test_name = test_name,
    };
    let test_dockerfile_name = test_name.to_string() + ".Dockerfile";
    let test_build_dockerfile = formatdoc!{
        "
            FROM {test_base_img}

            RUN echo '{test_name}' > /test.txt
        ",
        test_base_img = test_setup::TEST_BASE_IMG,
        test_name = test_name,
    };
    let fs_state = &hashmap!{
        "dock.yaml" => test_dock_yaml.as_str(),
        test_dockerfile_name.as_str() => test_build_dockerfile.as_str(),
    };
    let test_source_dir =
        test_setup::assert_create_dir(root_test_dir.to_string(), "templates");
    let test_templ_dir =
        test_setup::assert_create_dir(test_source_dir.clone(), "templ");
    test_setup::assert_write_fs_state(test_templ_dir.as_str(), fs_state);

    test_source_dir
}

fn assert_init_git_repo(dir: &str) {
    assert_git_cmds(
        dir,
        &[
            vec!["init"],
            vec!["config", "user.name", "Dev"],
            vec!["config", "user.email", "dev@example.com"],
        ],
    );

    assert_git_add_commit(dir, "Initial commit");
}

fn assert_git_add_commit(dir: &str, msg: &str) {
    assert_git_cmds(
        dir,
        &[
            vec!["add", "."],
            vec!["commit", "--message", msg],
        ],
    );
}

fn assert_git_cmds(dir: &str, arg_groups: &[Vec<&str>]) {
    for args in arg_groups {
        assert_run::assert_run_in_dir(dir, "git", args);
    }
}

fn assert_test_cmd(root_test_dir: &str, args: &[&str]) {
    run_test_cmd(root_test_dir, args).code(0);
}

// TODO Mostly duplicated from `crate::cli::run_in::success::run_test_cmd`.
fn run_test_cmd(root_test_dir: &str, args: &[&str]) -> Assert {
    let mut cmd = AssertCommand::cargo_bin(env!("CARGO_PKG_NAME"))
        .expect("couldn't create command for package binary");
    cmd.args(args);
    cmd.current_dir(root_test_dir);
    cmd.env_clear();

    if let Ok(v) = env::var(DOCK_HOSTPATHS_VAR_NAME) {
        cmd.env(DOCK_HOSTPATHS_VAR_NAME, v);
    }

    cmd.assert()
}

// TODO Duplicated from `crate::cli::run_in::success::run_test_cmd`.
const DOCK_HOSTPATHS_VAR_NAME: &str = "DOCK_HOSTPATHS";

#[test]
// Given (1) a Git repository `<source>` containing `<templ>`
//     AND (2) `<templ>` contains a dock file defining `<env>`
//     AND (3) `<templ>` contains a Dockerfile named `<env>.Dockerfile`
//     AND (4) `<env>.Dockerfile` creates a test file `<test>`
//     AND (5) a test directory `<dir>` exists
//     AND (6) `<dir>` contains a dock file
// When `dock init --source <source> <templ>` is run in `<dir>`
// Then (A) the command exits with code 2
//     AND (B) the command STDERR indicates `dock.yaml` already exists
//     AND (C) the command STDOUT is empty
fn init_exits_if_dock_file_exists() {
    let test_name = "init_exits_if_dock_file_exists";
    let root_test_dir = test_setup::assert_create_root_dir(test_name);
    // (1) (2) (3) (4)
    let test_source_dir = create_templates_dir(&root_test_dir, test_name);
    assert_init_git_repo(&test_source_dir);
    // (5)
    let test_dir = test_setup::assert_create_dir(root_test_dir, "dir");
    let dock_file = test_dir.clone() + "/dock.yaml";
    // (6)
    assert_run::assert_run_stdout("touch", &[dock_file.as_str()]);
    let source = format!("git:{test_source_dir}:master:.");

    let cmd_result =
        run_test_cmd(&test_dir, &["init", "--source", &source, "templ"]);

    cmd_result
        // (A)
        .code(2)
        // (B)
        .stderr(predicate_str::contains("already contains 'dock.yaml'"))
        // (C)
        .stdout("");
}

#[test]
// Given (1) a Git repository `<source>` containing `<templ>`
//     AND (2) `<templ>` contains a dock file defining `<env>`
//     AND (3) `<templ>` contains a Dockerfile named `<env>.Dockerfile`
//     AND (4) `<templ>/<env>.Dockerfile` is not empty
//     AND (5) a test directory `<dir>` exists
//     AND (6) `<dir>` contains a Dockerfile named `<env>.Dockerfile`
//     AND (7) `<dir>/<env>.Dockerfile` is empty
// When `dock init --source <source> <templ>` is run in `<dir>`
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the name of the dock file
//     AND (D) the command STDOUT contains `<env>.Dockerfile`
//     AND (E) the contents of `<dir>/<env>.Dockerfile` is unchanged
fn init_doesnt_overwrite_existing_files() {
    let test_name = "init_doesnt_overwrite_existing_files";
    let root_test_dir = test_setup::assert_create_root_dir(test_name);
    // (1) (2) (3) (4)
    let test_source_dir = create_templates_dir(&root_test_dir, test_name);
    assert_init_git_repo(&test_source_dir);
    // (5)
    let test_dir = test_setup::assert_create_dir(root_test_dir, "dir");
    let dockerfile_name = test_name.to_string() + ".Dockerfile";
    // (6)
    let dockerfile_path = format!("{test_dir}/{dockerfile_name}");
    // (7)
    assert_run::assert_run_stdout("touch", &[dockerfile_path.as_str()]);
    let source = format!("git:{test_source_dir}:master:.");

    let cmd_result =
        run_test_cmd(&test_dir, &["init", "--source", &source, "templ"]);

    let dockerfile_msg = format!("Skipped './{dockerfile_name}'");
    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(predicate_str::contains("Created './dock.yaml'"))
        // (D)
        .stdout(predicate_str::contains(dockerfile_msg));
    let dockerfile_contents =
        assert_run::assert_run_stdout("cat", &[dockerfile_path.as_str()]);
    // (E)
    assert_eq!(dockerfile_contents, "");
}

#[test]
// Given (1) a directory `<source>` containing `<templ>`
//     AND (2) `<templ>` contains a dock file defining `<env>`
//     AND (3) `<templ>` contains a Dockerfile named `<env>.Dockerfile`
//     AND (4) `<env>.Dockerfile` creates a test file `<test>`
//     AND (5) an empty test directory `<dir>` exists
//     AND (6) `dock init --source <source> <templ>` is run in `<dir>`
// When `dock run-in <env> cat <test>` is run in `<dir>`
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of `<test>`
fn init_with_dir_source() {
    let test_name = "init_with_dir_source";
    let root_test_dir = test_setup::assert_create_root_dir(test_name);
    // (1) (2) (3) (4)
    let test_source_dir = create_templates_dir(&root_test_dir, test_name);
    // (5)
    let test_dir = test_setup::assert_create_dir(root_test_dir, "dir");
    let source = format!("dir:{test_source_dir}:-:.");
    // (6)
    assert_test_cmd(&test_dir, &["init", "--source", &source, "templ"]);

    let cmd_result =
        run_test_cmd(&test_dir, &["run-in", test_name, "cat", "/test.txt"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(format!("{test_name}\n"));
}

#[test]
// Given (1) a directory `<source>` containing `<templ>`
//     AND (2) `<templ>` contains a non-empty directory
//     AND (3) an empty test directory `<dir>` exists
// When `dock init --source <source> <templ>` is run in `<dir>`
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
fn init_with_dir_in_template() {
    let test_name = "init_with_dir_in_template";
    let root_test_dir = test_setup::assert_create_root_dir(test_name);
    let test_source_dir =
        test_setup::assert_create_dir(root_test_dir.to_string(), "templates");
    // (1)
    let test_templ_dir =
        test_setup::assert_create_dir(test_source_dir.clone(), "templ");
    let fs_state = &hashmap!{"nonempty_dir/dummy" => ""};
    // (2)
    test_setup::assert_write_fs_state(test_templ_dir.as_str(), fs_state);
    // (3)
    let test_dir = test_setup::assert_create_dir(root_test_dir, "dir");
    let source = format!("dir:{test_source_dir}:-:.");

    let cmd_result =
        run_test_cmd(&test_dir, &["init", "--source", &source, "templ"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("");
}

#[test]
// Given (1) a directory `<source>` containing `<templ>`
//     AND (2) `<templ>` contains a non-empty directory `<nonempty>`
//     AND (3) a test directory `<dir>` exists
//     AND (4) `<dir>` contains a directory named `<nonempty>`
// When `dock init --source <source> <templ>` is run in `<dir>`
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
fn init_with_dir_in_template_and_dir_exists() {
    let test_name = "init_with_dir_in_template_and_dir_exists";
    let root_test_dir = test_setup::assert_create_root_dir(test_name);
    let test_source_dir =
        test_setup::assert_create_dir(root_test_dir.to_string(), "templates");
    // (1)
    let test_templ_dir =
        test_setup::assert_create_dir(test_source_dir.clone(), "templ");
    let fs_state = &hashmap!{"nonempty_dir/dummy" => ""};
    // (2)
    test_setup::assert_write_fs_state(test_templ_dir.as_str(), fs_state);
    // (3)
    let test_dir = test_setup::assert_create_dir(root_test_dir, "dir");
    // (4)
    fs::create_dir(format!("{test_dir}/nonempty_dir"))
        .expect("couldn't create \"non-empty\" directory");
    let source = format!("dir:{test_source_dir}:-:.");

    let cmd_result =
        run_test_cmd(&test_dir, &["init", "--source", &source, "templ"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("");
}

#[test]
// Given (1) a Git repository `<source>` containing `<templ>`
//     AND (2) `<templ>` contains a dock file defining `<env>`
//     AND (3) `<templ>` contains a Dockerfile named `<env>.Dockerfile`
//     AND (4) `<env>.Dockerfile` creates a test file `<test>`
//     AND (5) `<source>` is tagged as `v1`
//     AND (6) the contents of `<env>.Dockerfile` is changed in a new commit
//     AND (7) an empty test directory `<dir>` exists
//     AND (8) `dock init --source <source>:v1 <templ>` is run in `<dir>`
// When `dock run-in <env> cat <test>` is run in `<dir>`
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of `<test>`
fn init_from_old_git_tag() {
    let test_name = "init_from_old_git_tag";
    let root_test_dir = test_setup::assert_create_root_dir(test_name);
    // (1) (2) (3) (4)
    let test_source_dir = create_templates_dir(&root_test_dir, test_name);
    assert_init_git_repo(&test_source_dir);
    // (5)
    assert_run::assert_run_in_dir(&test_source_dir, "git", &["tag", "v1"]);
    update_templ_dockerfile(test_name, &test_source_dir);
    // (6)
    assert_git_add_commit(&test_source_dir, "Second commit");
    // (7)
    let test_dir = test_setup::assert_create_dir(root_test_dir, "dir");
    let source = format!("git:{test_source_dir}:v1:.");
    // (8)
    assert_test_cmd(&test_dir, &["init", "--source", &source, "templ"]);

    let cmd_result =
        run_test_cmd(&test_dir, &["run-in", test_name, "cat", "/test.txt"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(format!("{test_name}\n"));
}

fn update_templ_dockerfile(test_name: &str, test_source_dir: &str) {
    let test_dockerfile_name = test_name.to_string() + ".Dockerfile";
    let test_build_dockerfile = formatdoc!{
        "
            FROM {test_base_img}

            RUN echo '{test_name}.v2' > /test.txt
        ",
        test_base_img = test_setup::TEST_BASE_IMG,
        test_name = test_name,
    };
    let fs_state = &hashmap!{
        test_dockerfile_name.as_str() => test_build_dockerfile.as_str(),
    };
    let test_templ_dir = test_source_dir.to_string() + "/templ";
    test_setup::assert_write_fs_state(&test_templ_dir, fs_state);
}

#[test]
// Given (1) a Git repository `<source>` containing `<templ>`
//     AND (2) `<templ>` contains a dock file defining `<env>`
//     AND (3) `<templ>` contains a Dockerfile named `<env>.Dockerfile`
//     AND (4) `<env>.Dockerfile` creates a test file with contents
//     AND (5) `<source>` is tagged as `v1`
//     AND (6) the contents of `<env>.Dockerfile` is changed to `<test>`
//     AND (7) `<source>` is tagged as `v2`
//     AND (8) an empty test directory `<dir>` exists
//     AND (9) `dock init --source <source>:v2 <templ>` is run in `<dir>`
// When `dock run-in <env> cat <test>` is run in `<dir>`
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of `<test>`
fn init_from_new_git_tag() {
    let test_name = "init_from_new_git_tag";
    let root_test_dir = test_setup::assert_create_root_dir(test_name);
    // (1) (2) (3) (4)
    let test_source_dir = create_templates_dir(&root_test_dir, test_name);
    assert_init_git_repo(&test_source_dir);
    // (5)
    assert_run::assert_run_in_dir(&test_source_dir, "git", &["tag", "v1"]);
    update_templ_dockerfile(test_name, &test_source_dir);
    // (6)
    assert_git_add_commit(&test_source_dir, "Second commit");
    // (7)
    assert_run::assert_run_in_dir(&test_source_dir, "git", &["tag", "v2"]);
    // (8)
    let test_dir = test_setup::assert_create_dir(root_test_dir, "dir");
    let source = format!("git:{test_source_dir}:v2:.");
    // (9)
    assert_test_cmd(&test_dir, &["init", "--source", &source, "templ"]);

    let cmd_result =
        run_test_cmd(&test_dir, &["run-in", test_name, "cat", "/test.txt"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(format!("{test_name}.v2\n"));
}

#[test]
// Given (1) a directory `<source>` containing `a/b/<templ>`
//     AND (2) `<templ>` contains a dock file defining `<env>`
//     AND (3) `<templ>` contains a Dockerfile named `<env>.Dockerfile`
//     AND (4) `<env>.Dockerfile` creates a test file `<test>`
//     AND (5) an empty test directory `<dir>` exists
//     AND (6) `dock init --source <source>:./a/b <templ>` is run in `<dir>`
// When `dock run-in <env> cat <test>` is run in `<dir>`
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT contains the contents of `<test>`
fn init_with_subdir() {
    let test_name = "init_with_subdir";
    let root_test_dir = test_setup::assert_create_root_dir(test_name);
    // (1) (2) (3) (4)
    let test_source_dir = create_templates_subdir(&root_test_dir, test_name);
    // (5)
    let test_dir = test_setup::assert_create_dir(root_test_dir, "dir");
    let source = format!("dir:{test_source_dir}:-:./a/b");
    // (6)
    assert_test_cmd(&test_dir, &["init", "--source", &source, "templ"]);

    let cmd_result =
        run_test_cmd(&test_dir, &["run-in", test_name, "cat", "/test.txt"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout(format!("{test_name}\n"));
}

// TODO Mostly duplicated from `create_templates_dir`.
fn create_templates_subdir(root_test_dir: &str, test_name: &str) -> String {
    let test_dock_yaml = formatdoc!{
        "
            schema_version: '0.1'
            organisation: org
            project: proj
            default_shell_env: {test_name}

            environments:
              {test_name}:
                mount_local:
                - user
        ",
        test_name = test_name,
    };
    let test_dockerfile_name = test_name.to_string() + ".Dockerfile";
    let test_build_dockerfile = formatdoc!{
        "
            FROM {test_base_img}

            RUN echo '{test_name}' > /test.txt
        ",
        test_base_img = test_setup::TEST_BASE_IMG,
        test_name = test_name,
    };
    let fs_state = &hashmap!{
        "dock.yaml" => test_dock_yaml.as_str(),
        test_dockerfile_name.as_str() => test_build_dockerfile.as_str(),
    };
    let test_source_dir =
        test_setup::assert_create_dir(root_test_dir.to_string(), "templates");

    let mut dir = test_source_dir.clone();
    for name in &["a", "b"] {
        dir = test_setup::assert_create_dir(dir, name);
    }

    let test_templ_dir = test_setup::assert_create_dir(dir, "templ");
    test_setup::assert_write_fs_state(test_templ_dir.as_str(), fs_state);

    test_source_dir
}
