// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

pub mod build;

use std::process::Command;
use std::str;

use crate::assert_run;

pub fn assert_kill_image_container(image_tagged_name: &str) {
    let ids = containers_for_image(image_tagged_name);
    if ids.is_empty() {
        return;
    }

    let ids: Vec<&str> =
        ids
            .iter()
            .map(AsRef::as_ref)
            .collect();

    let mut args = vec!["kill"];
    args.extend(ids);

    assert_run::assert_run_stdout("docker", &args);
}

// `containers_for_image` returns IDs of existing containers that are descended
// from the `tagged_name` image.
fn containers_for_image(tagged_name: &str) -> Vec<String> {
    assert_run::assert_run_stdout_lines(
        "docker",
        &[
            "ps",
            "--all",
            "--quiet",
            &format!("--filter=ancestor={tagged_name}"),
        ],
    )
}

pub fn assert_remove_image(image_tagged_name: &str) {
    let mut cmd = Command::new("docker");
    cmd.args(vec!["rmi", image_tagged_name]);
    cmd.env_clear();

    let output = cmd.output()
        .unwrap_or_else(|_| panic!(
            "couldn't get output for `docker rmi {image_tagged_name}`",
        ));

    if output.status.success() {
        return
    }

    let stderr = str::from_utf8(&output.stderr)
        .unwrap_or_else(|_| panic!(
            "couldn't decode STDERR for `docker rmi {image_tagged_name}`",
        ));

    let allowable_stderr = format!(
        "Error response from daemon: No such image: {image_tagged_name}\n",
    );
    assert!(stderr == allowable_stderr, "unexpected STDERR: {output:?}");
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
    assert_run::assert_run_stdout_lines(
        "docker",
        &["images", "--format={{.Repository}}:{{.Tag}}"],
    )
}

// `assert_no_containers_from_image` asserts that no containers exist that are
// descended from the `tagged_name` image.
pub fn assert_no_containers_from_image(tagged_name: &str) {
    let ids = containers_for_image(tagged_name);
    assert!(ids.is_empty(), "containers were found for '{tagged_name}'");
}

pub fn assert_remove_volume(name: &str) {
    let mut cmd = Command::new("docker");
    cmd.args(vec!["volume", "rm", name]);
    cmd.env_clear();

    let output = cmd.output()
        .unwrap_or_else(|_| panic!(
            "couldn't get output for `docker volume rm {name}`",
        ));

    if output.status.success() {
        return
    }

    let stderr = str::from_utf8(&output.stderr)
        .unwrap_or_else(|_| panic!(
            "couldn't decode STDERR for `docker volume rm {name}`",
        ));

    let allowable_stderr = format!("Error: No such volume: {name}\n");
    assert!(stderr == allowable_stderr, "unexpected STDERR: {output:?}");
}
