// Copyright 2021-2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::str;

use crate::assert_run;
use crate::docker;
use crate::docker::build::DockerBuild;
use crate::line_matcher;
use crate::line_matcher::LineMatcher;
use crate::test_setup;
use crate::test_setup::Definition;
use crate::test_setup::References;

use crate::assert_cmd::assert::Assert as AssertOutput;
use crate::assert_cmd::Command as AssertCommand;

#[test]
// Given (1) the Dockerfile creates a test file
//     AND (2) the target image doesn't already exist
// When the `rebuild` subcommand is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT is formatted correctly
//     AND (D) the target image exists
//     AND (E) a container created from the target image contains the test file
fn rebuild_creates_image_if_none() {
    // (1)
    let test_name = "rebuild_creates_image_if_none";
    let test = test_setup::assert_apply(&Definition{
        name: test_name,
        dockerfile_steps: indoc!{"
            COPY test.txt /
        "},
        fs: &hashmap!{"test.txt" => test_name},
    });
    // (2)
    docker::assert_remove_image(&test.image_tagged_name);
    let mut cmd = new_test_cmd(test.dir, &test.image_tagged_name);

    let cmd_result = cmd.assert();

    // (A) (B) (C)
    assert_docker_build(cmd_result, &test.image_tagged_name);
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
    // (E)
    assert_match_docker_run_stdout(
        &test.image_tagged_name,
        &["cat", "test.txt"],
        test_name,
    );
}

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

fn assert_match_docker_run_stdout(
    img: &str,
    run_args: &[&str],
    exp_stdout: &str,
) {
    let args = [&["run", "--rm", img], run_args].concat();
    let act_stdout = assert_run::assert_run_stdout("docker", &args);
    assert_eq!(exp_stdout, act_stdout);
}

fn assert_docker_build(cmd_result: AssertOutput, tagged_name: &str)
    -> DockerBuild
{
    let cmd_result = cmd_result.code(0);
    let cmd_result = cmd_result.stderr("");

    let stdout = new_str_from_cmd_stdout(&cmd_result);
    let maybe_build = assert_docker_build_stdout(stdout);

    if let Some(build) = maybe_build {
        assert_eq!(build.tagged_name(), tagged_name);

        build
    } else {
        panic!("build was unsuccessful: {}", stdout);
    }
}

pub fn new_str_from_cmd_stdout(cmd_result: &AssertOutput) -> &str {
    let stdout_bytes = &cmd_result.get_output().stdout;

    str::from_utf8(stdout_bytes)
        .expect("couldn't decode STDOUT")
}

pub fn assert_docker_build_stdout(stdout: &str) -> Option<DockerBuild> {
    let mut lines = LineMatcher::new(stdout);
    let result = DockerBuild::parse_from_stdout(&mut lines);

    match result {
        Ok(v) => {
            v
        },
        Err(e) => {
            let lnum = lines.line_num();
            let msg = line_matcher::render_match_error(stdout, lnum, &e);
            panic!("{}", msg);
        },
    }
}

#[test]
// Given (1) the target image already exists with a certain ID
//     AND (2) a container created from the image contains a test file
//     AND (3) the test file is updated in the context
// When the `rebuild` subcommand is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT is formatted correctly
//     AND (D) the target image has a new ID
//     AND (E) the old image ID doesn't exist
//     AND (F) a container created from the target image contains the new file
fn rebuild_replaces_old_image() {
    let test_name = "rebuild_replaces_old_image";
    let mut test_dir_layout = hashmap!{"test.txt" => test_name};
    // (1)
    let (test, old_image_id) = rebuild_img(&Definition{
        name: test_name,
        dockerfile_steps: indoc!{"
            COPY test.txt /
        "},
        fs: &test_dir_layout,
    });
    // (2)
    assert_match_docker_run_stdout(
        &test.image_tagged_name,
        &["cat", "test.txt"],
        test_name,
    );
    // (3)
    let new_test_name = &(test_name.to_owned() + ".update");
    test_dir_layout.insert("test.txt", new_test_name);
    let updated_test = test_setup::assert_apply(&Definition{
        name: new_test_name,
        dockerfile_steps: indoc!{"
            COPY test.txt /
        "},
        fs: &test_dir_layout,
    });
    // We rebuild the main test image, but we build it in the context of the
    // updated test directory. This allows us to validate the directory
    // contents for the purposes of debugging.
    let mut cmd = new_test_cmd(updated_test.dir, &test.image_tagged_name);

    let cmd_result = cmd.assert();

    // (A) (B) (C)
    let new_build = assert_docker_build(cmd_result, &test.image_tagged_name);
    // (D)
    assert_ne!(new_build.img_id(), old_image_id);
    // (E)
    assert!(!get_local_docker_image_ids().contains(&old_image_id));
    // (F)
    assert_match_docker_run_stdout(
        &test.image_tagged_name,
        &["cat", "test.txt"],
        new_test_name,
    );
}

pub fn rebuild_img(test_defn: &Definition) -> (References, String) {
    let test = test_setup::assert_apply(test_defn);
    let mut cmd = new_test_cmd(test.dir.clone(), &test.image_tagged_name);
    let build = assert_docker_build(cmd.assert(), &test.image_tagged_name);

    (test, build.img_id().to_string())
}

fn get_local_docker_image_ids() -> Vec<String> {
    let args = &["images", "--quiet"];
    let stdout = assert_run::assert_run_stdout("docker", args);

    stdout
        .split_ascii_whitespace()
        .map(ToOwned::to_owned)
        .collect()
}

#[test]
// Given (1) the target image already exists with a certain ID
// When the `rebuild` subcommand is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT is formatted correctly
//     AND (D) the target image has the same ID
fn rebuild_unchanged_context_doesnt_replace_image() {
    let test_name = "rebuild_unchanged_context_doesnt_replace_image";
    // (1)
    let (test, old_image_id) = rebuild_img(&Definition{
        name: test_name,
        dockerfile_steps: indoc!{"
            RUN touch test.txt
        "},
        fs: &hashmap!{},
    });
    let mut cmd = new_test_cmd(test.dir, &test.image_tagged_name);

    cmd.assert();

    // (A) (B) (C)
    let new_build = assert_docker_build(cmd.assert(), &test.image_tagged_name);
    // (D)
    assert_eq!(new_build.img_id(), old_image_id);
}

#[test]
// Given (1) a valid Dockerfile named `test.Dockerfile`
//     AND (2) the target image doesn't already exist
// When the `rebuild` subcommand is run with a `--file` argument
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT is formatted correctly
fn file_argument() {
    // (1)
    let test = test_setup::assert_apply_with_dockerfile_name(
        "test.Dockerfile",
        &Definition{
            name: "file_argument",
            dockerfile_steps: "",
            fs: &hashmap!{},
        },
    );
    // (2)
    docker::assert_remove_image(&test.image_tagged_name);
    let mut cmd = new_test_cmd(test.dir, &test.image_tagged_name);
    cmd.arg("--file=test.Dockerfile");

    cmd.assert();

    // (A) (B) (C)
    assert_docker_build(cmd.assert(), &test.image_tagged_name);
}
