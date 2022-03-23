// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

pub mod build;

use std::process::Command;
use std::str;

use crate::assert_run;

pub fn assert_remove_image(image_tagged_name: &str) {
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

pub fn assert_image_exists(image_tagged_name: &str) {
    assert!(image_exists(image_tagged_name));
}

pub fn image_exists(image_tagged_name: &str) -> bool {
    assert_get_local_image_tagged_names()
        .contains(&image_tagged_name.to_owned())
}

pub fn assert_image_doesnt_exist(image_tagged_name: &str) {
    assert!(!image_exists(image_tagged_name));
}

// `assert_get_local_image_tagged_names` returns a `Vec` of the tagged image
// names for all the Docker images found on the local Docker server.
pub fn assert_get_local_image_tagged_names() -> Vec<String> {
    let stdout = assert_run::assert_run_stdout(
        "docker",
        &["images", "--format={{.Repository}}:{{.Tag}}"],
    );

    stdout
        .split_ascii_whitespace()
        .map(ToOwned::to_owned)
        .collect()
}

// `assert_no_containers_from_image` asserts that no containers exist that are
// descended from the `tagged_name` image.
pub fn assert_no_containers_from_image(tagged_name: &str) {
    let stdout = assert_run::assert_run_stdout(
        "docker",
        &[
            "ps",
            "--all",
            "--quiet",
            &format!("--filter=ancestor={}", tagged_name),
        ],
    );

    assert!(stdout == "", "stdout is not empty: {}", stdout);
}
