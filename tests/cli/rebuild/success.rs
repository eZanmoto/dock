// Copyright 2021-2024 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::path::PathBuf;
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
    let mut cmd = new_test_cmd(&test.dir, &test.image_tagged_name);

    let cmd_result = cmd.assert();

    // (A) (B) (C)
    assert_build_result(cmd_result, &test.image_tagged_name);
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
    root_test_dir: &str,
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

fn assert_match_docker_run_stdout(
    img: &str,
    run_args: &[&str],
    exp_stdout: &str,
) {
    let args = [&["run", "--rm", img], run_args].concat();
    let act_stdout = assert_run::assert_run_stdout("docker", &args);
    assert_eq!(exp_stdout, act_stdout);
}

fn assert_build_result(cmd_result: AssertOutput, tagged_name: &str)
    -> DockerBuild
{
    let cmd_result = cmd_result.code(0);

    let stderr = new_str_from_cmd_stderr(&cmd_result);
    let maybe_build = assert_docker_build_stderr(stderr);

    if let Some(build) = maybe_build {
        assert_eq!(build.tagged_name(), tagged_name);

        build
    } else {
        panic!("build was unsuccessful: {stderr}");
    }
}

pub fn new_str_from_cmd_stderr(cmd_result: &AssertOutput) -> &str {
    let stderr_bytes = &cmd_result.get_output().stderr;

    str::from_utf8(stderr_bytes)
        .expect("couldn't decode STDERR")
}

pub fn assert_docker_build_stderr(stderr: &str) -> Option<DockerBuild> {
    let mut lines = LineMatcher::new(stderr);
    let result = DockerBuild::parse_from_stderr(&mut lines);

    match result {
        Ok(v) => {
            v
        },
        Err(e) => {
            let lnum = lines.line_num();
            let msg = line_matcher::render_match_error(stderr, lnum, &e);
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
    let mut cmd = new_test_cmd(&updated_test.dir, &test.image_tagged_name);

    let cmd_result = cmd.assert();

    // (A) (B) (C)
    let new_build = assert_build_result(cmd_result, &test.image_tagged_name);
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
    let mut cmd = new_test_cmd(&test.dir, &test.image_tagged_name);
    let build = assert_build_result(cmd.assert(), &test.image_tagged_name);

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
    let mut cmd = new_test_cmd(&test.dir, &test.image_tagged_name);

    cmd.assert();

    // (A) (B) (C)
    let new_build = assert_build_result(cmd.assert(), &test.image_tagged_name);
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
    let mut cmd = new_test_cmd(&test.dir, &test.image_tagged_name);
    cmd.arg("--file=test.Dockerfile");

    cmd.assert();

    // (A) (B) (C)
    assert_build_result(cmd.assert(), &test.image_tagged_name);
}

#[test]
// Given (1) the Dockerfile creates a test file
//     AND (2) the target image doesn't already exist
//     AND (3) the command is set up to read the Dockerfile through STDIN
// When the `rebuild` subcommand is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT is formatted correctly
//     AND (D) the target image exists
//     AND (E) a container created from the target image contains the test file
fn pass_dockerfile_through_stdin() {
    let test_name = "pass_dockerfile_through_stdin";
    let test = test_setup::assert_apply(&Definition{
        name: test_name,
        // (1)
        dockerfile_steps: &formatdoc!{
            "
                RUN echo -n '{test_name}' > /test.txt
            ",
            test_name = test_name,
        },
        fs: &hashmap!{},
    });
    // (2)
    docker::assert_remove_image(&test.image_tagged_name);
    let dockerfile = PathBuf::from(format!("{}/Dockerfile", test.dir));
    // (3)
    let mut cmd = new_test_cmd_with_stdin(
        Stdin::File(dockerfile),
        &test.image_tagged_name,
    );

    let cmd_result = cmd.assert();

    // (A) (B) (C)
    assert_build_result(cmd_result, &test.image_tagged_name);
    // (D)
    docker::assert_image_exists(&test.image_tagged_name);
    // (E)
    assert_match_docker_run_stdout(
        &test.image_tagged_name,
        &["cat", "test.txt"],
        test_name,
    );
}

enum Stdin{
    File(PathBuf),
    Str(String),
}

fn new_test_cmd_with_stdin(stdin: Stdin, image_tagged_name: &str)
    -> AssertCommand
{
    let mut cmd = AssertCommand::cargo_bin(env!("CARGO_PKG_NAME"))
        .expect("couldn't create command for package binary");
    cmd.args(vec!["rebuild", image_tagged_name, "-"]);
    cmd.env_clear();
    // We set `HOME` because if unset then Docker BuildKit will create a
    // `.docker` directory in the working directory during builds.
    cmd.env("HOME", env!("HOME"));

    match stdin {
        Stdin::File(path) => {
            cmd.pipe_stdin(path)
                .expect("couldn't pipe STDIN");
        },
        Stdin::Str(s) => {
            cmd.write_stdin(s);
        },
    }

    cmd
}

#[test]
// Given (1) an image `<base_img>`
//     AND (2) a container using `<base_img>`
//     AND (3) a Dockerfile that only contains a `FROM <base_img>` instruction
//     AND (4) an image `<test_img>` created from the Dockerfile
//     AND (5) the Dockerfile is updated with more instructions
// When `rebuild <test_img>` is run with the updated Dockerfile
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT is formatted correctly
//     AND (D) the target image exists
//
// This test checks an issue with the rebuild approach that removes the old
// image by ID. If an image is created with a Dockerfile that only has a `FROM`
// instruction, then the image created from this Dockerfile will have the same
// ID as the image in the `FROM` instruction. If the Dockerfile is then
// updated and rebuilt, then during cleanup, `rebuild` will try and remove the
// original image. This is undesirable in itself, but if a container exists
// that is based on the original image, then this will prevent the removal and
// will cause `rebuild` to return an error.
fn cleanup_succeeds_even_if_image_not_removed() {
    let test_name = "cleanup_succeeds_even_if_image_not_removed";
    // (1)
    let base_img = &test_setup::TEST_BASE_IMG;
    let args = &["container", "create", base_img];
    // (2)
    let container_id = assert_run::assert_run_stdout("docker", args);
    defer!{
        let rm_args = &["container", "rm", container_id.trim_end()];
        assert_run::assert_run_stdout("docker", rm_args);
    }
    // (3)
    let mut dockerfile = format!("FROM {base_img}\n");
    let stdin = Stdin::Str(dockerfile.clone());
    let test_img = test_setup::test_image_tagged_name(test_name);
    // (4)
    assert_cmd_success(new_test_cmd_with_stdin(stdin, &test_img));
    // (5)
    dockerfile += "RUN true\n";
    let mut cmd = new_test_cmd_with_stdin(Stdin::Str(dockerfile), &test_img);

    let cmd_result = cmd.assert();

    // (A) (B) (C)
    assert_build_result(cmd_result, &test_img);
    // (D)
    docker::assert_image_exists(&test_img);
}

fn assert_cmd_success(mut cmd: AssertCommand) {
    cmd.assert().success();
}
