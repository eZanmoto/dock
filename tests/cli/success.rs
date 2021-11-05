// Copyright 2021 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

// NOTE Many of the tests contain a test file file called `test.txt` which
// contains the test name as content. In addition to being useful for verifying
// content, having different content in these files prevents Docker from
// reusing cached image layers across tests.

use std::process::Command;
use std::str;
use std::str::Lines;

use crate::test_setup;

use crate::assert_cmd::Command as AssertCommand;
use crate::assert_cmd::assert::Assert as AssertOutput;

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
    let test = test_setup::create(
        test_name,
        &hashmap!{
            "Dockerfile" => indoc!{"
                FROM alpine:3.14.2
                COPY test.txt /
            "},
            "test.txt" => test_name,
        },
    );
    // (2)
    assert_docker_rmi(&test.image_tagged_name);
    let mut cmd = new_test_cmd(test.dir, &test.image_tagged_name);

    let cmd_result = cmd.assert();

    // (A) (B) (C)
    assert_docker_build(cmd_result, &test.image_tagged_name);
    // (D)
    assert_cmd_stdout(
        "docker",
        &["image", "inspect", &test.image_tagged_name],
    );
    // (E)
    assert_match_docker_run_stdout(
        &test.image_tagged_name,
        &["cat", "test.txt"],
        test_name,
    );
}

fn assert_docker_rmi(image_tagged_name: &str) {
    let mut cmd = Command::new("docker");
    cmd.args(vec!["rmi", &image_tagged_name]);
    cmd.env_clear();

    let output = cmd.output()
        .unwrap_or_else(|_| panic!(
            "couldn't get output for `docker rmi {}`",
            image_tagged_name,
        ));

    if output.status.success() {
        return
    }

    let stderr = str::from_utf8(&output.stderr)
        .unwrap_or_else(|_| panic!(
            "couldn't decode stderr for `docker rmi {}`",
            image_tagged_name,
        ));

    let allowable_stderr =
        format!("Error: No such image: {}\n", image_tagged_name);
    if stderr != allowable_stderr {
        panic!("unexpected stderr: {:?}", output);
    }
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
    let act_stdout = assert_cmd_stdout("docker", &args);
    assert_eq!(exp_stdout, act_stdout);
}

fn assert_cmd_stdout(prog: &str, args: &[&str]) -> String {
    let mut cmd = AssertCommand::new(prog);
    cmd.args(args);
    cmd.env_clear();

    let result = cmd.assert().code(0);
    let stdout = &result.get_output().stdout;

    str::from_utf8(&stdout)
        .expect("couldn't decode STDOUT as UTF-8")
        .to_string()
}

fn assert_docker_build(
    cmd_result: AssertOutput,
    img_tagged_name: &str,
) -> DockerBuild {
    let cmd_result = cmd_result.code(0);
    let cmd_result = cmd_result.stderr("");

    let stdout_bytes = &cmd_result.get_output().stdout;

    let stdout = str::from_utf8(stdout_bytes)
        .expect("couldn't decode STDOUT");

    assert_parse_docker_build_stdout(&mut stdout.lines(), &img_tagged_name)
}

fn assert_parse_docker_build_stdout(stdout: &mut Lines, tagged_name: &str)
    -> DockerBuild
{
    let mut line = stdout.next().unwrap();
    let exp_line = "Sending build context to Docker daemon";
    assert!(line.starts_with(exp_line), "unexpected prefix: {}", line);

    let mut last_layer_id: Option<String> = None;
    let mut layers = vec![];
    loop {
        line = stdout.next().unwrap();
        if let Some(id) = last_layer_id {
            if line.starts_with(&("Successfully built ".to_owned() + &id)) {
                break;
            }
        }

        assert!(line.starts_with("Step "), "unexpected prefix: {}", line);
        line = stdout.next().unwrap();

        if line.starts_with(" ---> Using cache") {
            line = stdout.next().unwrap();
        }

        if line.starts_with(" ---> Running in ") {
            line = stdout.next().unwrap();

            while !line.starts_with(" ---> ") {
                line = stdout.next().unwrap();
            }
        }

        let layer_id = line.strip_prefix(" ---> ").unwrap().to_string();
        last_layer_id = Some(layer_id.clone());
        layers.push(DockerBuildLayer{id: layer_id});
    }
    line = stdout.next().unwrap();

    assert_eq!(line, "Successfully tagged ".to_owned() + tagged_name);

    assert_eq!(stdout.next(), None);

    DockerBuild{layers}
}

#[derive(Debug)]
struct DockerBuild {
    layers: Vec<DockerBuildLayer>
}

impl DockerBuild {
    fn img_id(&self) -> String {
        self.layers
            .last()
            .unwrap()
            .id
            .clone()
    }
}

#[derive(Debug)]
struct DockerBuildLayer {
    id: String,
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
    let mut test_dir_layout = hashmap!{
        "Dockerfile" => indoc!{"
            FROM alpine:3.14.2
            COPY test.txt /
        "},
        "test.txt" => test_name,
    };
    let test = test_setup::create(test_name, &test_dir_layout);
    let mut cmd = new_test_cmd(test.dir, &test.image_tagged_name);
    let build = assert_docker_build(cmd.assert(), &test.image_tagged_name);
    // (1)
    let old_image_id = build.img_id();
    // (2)
    assert_match_docker_run_stdout(
        &test.image_tagged_name,
        &["cat", "test.txt"],
        test_name,
    );
    // (3)
    let new_test_name = &(test_name.to_owned() + ".update");
    test_dir_layout.insert("test.txt", new_test_name);
    let updated_test = test_setup::create(new_test_name, &test_dir_layout);
    // We rebuild the main test image, but we build it in the context of the
    // updated test directory. This allows us to validate the directory
    // contents for the purposes of debugging.
    let mut updated_cmd =
        new_test_cmd(updated_test.dir, &test.image_tagged_name);

    let cmd_result = updated_cmd.assert();

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

fn get_local_docker_image_ids() -> Vec<String> {
    let stdout = assert_cmd_stdout("docker", &["images", "--quiet"]);

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
    let test = test_setup::create(
        test_name,
        &hashmap!{
            "Dockerfile" => indoc!{"
                FROM alpine:3.14.2
                RUN touch test.txt
            "},
        },
    );
    let mut cmd = new_test_cmd(test.dir, &test.image_tagged_name);
    let build = assert_docker_build(cmd.assert(), &test.image_tagged_name);
    // (1)
    let old_image_id = build.img_id();

    cmd.assert();

    // (A) (B) (C)
    let new_build = assert_docker_build(cmd.assert(), &test.image_tagged_name);
    // (D)
    assert_eq!(new_build.img_id(), old_image_id);
}

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
