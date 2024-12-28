// Copyright 2024 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::env;
use std::str;

use crate::assert_run;
use crate::docker;
use crate::test_setup;

use crate::assert_cmd::assert::Assert;
use crate::assert_cmd::Command as AssertCommand;

#[test]
// Given (1) the dock file defines environments called `<env1>` and `<env2>`
//     AND (2) `<env1>` and `<env2>` define cache volumes
//     AND (3) the image for `<env1>` exists
//     AND (4) the cache volume for `<env1>` exists
//     AND (5) the image for `<env2>` exists
//     AND (6) the cache volume for `<env2>` exists
// When `clean` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT is empty
//     AND (D) the image for `<env1>` doesn't exist
//     AND (E) the image for `<env2>` doesn't exist
//     AND (F) the cache volume for `<env1>` doesn't exist
//     AND (G) the cache volume for `<env2>` doesn't exist
fn clean_removes_images_and_volumes() {
    let test_name = "clean_removes_images_and_volumes";
    let root_test_dir = test_setup::assert_create_root_dir(test_name);
    // (1) (2)
    let setup = create_templates_dir(&root_test_dir, test_name);
    assert_noop_run(&root_test_dir, &setup.env1);
    // (3)
    docker::assert_image_exists(&setup.env1_img_tagged_name);
    // (4)
    docker::assert_volume_exists(&setup.env1_cache_vol);
    assert_noop_run(&root_test_dir, &setup.env2);
    // (5)
    docker::assert_image_exists(&setup.env2_img_tagged_name);
    // (6)
    docker::assert_volume_exists(&setup.env2_cache_vol);

    let cmd_result = run_test_cmd(&root_test_dir, &["clean"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout("");
    // (D)
    docker::assert_image_doesnt_exist(&setup.env1_img_tagged_name);
    // (E)
    docker::assert_image_doesnt_exist(&setup.env2_img_tagged_name);
    // (F)
    docker::assert_volume_doesnt_exist(&setup.env1_cache_vol);
    // (G)
    docker::assert_volume_doesnt_exist(&setup.env2_cache_vol);
}

fn create_templates_dir(dir: &str, test_name: &str) -> TestSetup {
    let test_dock_yaml = formatdoc!{
        "
            schema_version: '0.1'
            organisation: org
            project: proj
            default_shell_env: {test_name}1

            environments:
              {test_name}1:
                cache_volumes: {{ test: /test }}

              {test_name}2:
                cache_volumes: {{ test: /test }}
        ",
        test_name = test_name,
    };
    let test_dockerfile1_name = test_name.to_string() + "1.Dockerfile";
    let test_build1_dockerfile = formatdoc!{
        "
            FROM {test_base_img}

            RUN echo '{test_name}1' > /test.txt
        ",
        test_base_img = test_setup::TEST_BASE_IMG,
        test_name = test_name,
    };
    let test_dockerfile2_name = test_name.to_string() + "2.Dockerfile";
    let test_build2_dockerfile = formatdoc!{
        "
            FROM {test_base_img}

            RUN echo '{test_name}2' > /test.txt
        ",
        test_base_img = test_setup::TEST_BASE_IMG,
        test_name = test_name,
    };
    let fs_state = &hashmap!{
        "dock.yaml" => test_dock_yaml.as_str(),
        test_dockerfile1_name.as_str() => test_build1_dockerfile.as_str(),
        test_dockerfile2_name.as_str() => test_build2_dockerfile.as_str(),
    };
    test_setup::assert_write_fs_state(dir, fs_state);

    TestSetup{
        env1: format!("{test_name}1"),
        env1_img_tagged_name: format!("org/proj.{test_name}1:latest"),
        env1_cache_vol: format!("org.proj.{test_name}1.cache.test"),
        env2: format!("{test_name}2"),
        env2_img_tagged_name: format!("org/proj.{test_name}2:latest"),
        env2_cache_vol: format!("org.proj.{test_name}2.cache.test"),
    }
}

struct TestSetup {
    env1: String,
    env1_img_tagged_name: String,
    env1_cache_vol: String,
    env2: String,
    env2_img_tagged_name: String,
    env2_cache_vol: String,
}

// TODO Mostly duplicated from `crate::cli::run_in::success::run_test_cmd`.
fn run_test_cmd(dir: &str, args: &[&str]) -> Assert {
    let mut cmd = AssertCommand::cargo_bin(env!("CARGO_PKG_NAME"))
        .expect("couldn't create command for package binary");
    cmd.args(args);
    cmd.current_dir(dir);
    cmd.env_clear();

    // We set `HOME` because if unset then Docker BuildKit will create a
    // `.docker` directory in the working directory during builds.
    cmd.env("HOME", env!("HOME"));

    if let Ok(v) = env::var(DOCK_HOSTPATHS_VAR_NAME) {
        cmd.env(DOCK_HOSTPATHS_VAR_NAME, v);
    }

    cmd.assert()
}

// TODO Duplicated from `crate::cli::run_in::success::run_test_cmd`.
const DOCK_HOSTPATHS_VAR_NAME: &str = "DOCK_HOSTPATHS";

fn assert_noop_run(dir: &str, env: &str) {
    run_test_cmd(dir, &["run-in", env, "true"]).code(0);
}

#[test]
// Given (1) the dock file defines environments called `<env1>` and `<env2>`
//     AND (2) `<env1>` and `<env2>` define cache volumes
//     AND (3) the image for `<env1>` doesn't exist
//     AND (4) the cache volume for `<env1>` doesn't exist
//     AND (5) the image for `<env2>` exists
//     AND (6) the cache volume for `<env2>` exists
// When `clean` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT is empty
//     AND (D) the image for `<env1>` doesn't exist
//     AND (E) the image for `<env2>` doesn't exist
//     AND (F) the cache volume for `<env1>` doesn't exist
//     AND (G) the cache volume for `<env2>` doesn't exist
fn clean_ignores_missing_images_and_volumes() {
    let test_name = "clean_ignores_missing_images_and_volumes";
    let root_test_dir = test_setup::assert_create_root_dir(test_name);
    // (1) (2)
    let setup = create_templates_dir(&root_test_dir, test_name);
    // (3)
    docker::assert_image_doesnt_exist(&setup.env1_img_tagged_name);
    // (4)
    docker::assert_volume_doesnt_exist(&setup.env1_cache_vol);
    assert_noop_run(&root_test_dir, &setup.env2);
    // (5)
    docker::assert_image_exists(&setup.env2_img_tagged_name);
    // (6)
    docker::assert_volume_exists(&setup.env2_cache_vol);

    let cmd_result = run_test_cmd(&root_test_dir, &["clean"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout("");
    // (D)
    docker::assert_image_doesnt_exist(&setup.env1_img_tagged_name);
    // (E)
    docker::assert_image_doesnt_exist(&setup.env2_img_tagged_name);
    // (F)
    docker::assert_volume_doesnt_exist(&setup.env1_cache_vol);
    // (G)
    docker::assert_volume_doesnt_exist(&setup.env2_cache_vol);
}

#[test]
// Given (1) the dock file defines environments called `<env1>` and `<env2>`
//     AND (2) `<env1>` and `<env2>` define cache volumes
//     AND (3) the image for `<env1>` exists
//     AND (4) the cache volume for `<env1>` exists
//     AND (5) the image for `<env2>` exists
//     AND (6) the cache volume for `<env2>` exists
// When `clean --skip-images` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT is empty
//     AND (D) the image for `<env1>` exists
//     AND (E) the image for `<env2>` exists
//     AND (F) the cache volume for `<env1>` doesn't exist
//     AND (G) the cache volume for `<env2>` doesn't exist
fn clean_skip_images() {
    let test_name = "clean_skip_images";
    let root_test_dir = test_setup::assert_create_root_dir(test_name);
    // (1) (2)
    let setup = create_templates_dir(&root_test_dir, test_name);
    assert_noop_run(&root_test_dir, &setup.env1);
    // (3)
    docker::assert_image_exists(&setup.env1_img_tagged_name);
    // (4)
    docker::assert_volume_exists(&setup.env1_cache_vol);
    assert_noop_run(&root_test_dir, &setup.env2);
    // (5)
    docker::assert_image_exists(&setup.env2_img_tagged_name);
    // (6)
    docker::assert_volume_exists(&setup.env2_cache_vol);

    let cmd_result = run_test_cmd(&root_test_dir, &["clean", "--skip-images"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout("");
    // (D)
    docker::assert_image_exists(&setup.env1_img_tagged_name);
    // (E)
    docker::assert_image_exists(&setup.env2_img_tagged_name);
    // (F)
    docker::assert_volume_doesnt_exist(&setup.env1_cache_vol);
    // (G)
    docker::assert_volume_doesnt_exist(&setup.env2_cache_vol);
}

#[test]
// Given (1) the dock file defines environments called `<env1>` and `<env2>`
//     AND (2) `<env1>` and `<env2>` define cache volumes
//     AND (3) the image for `<env1>` exists
//     AND (4) the cache volume for `<env1>` exists
//     AND (5) the image for `<env2>` exists
//     AND (6) the cache volume for `<env2>` exists
// When `clean --skip-volumes` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT is empty
//     AND (D) the image for `<env1>` doesn't exist
//     AND (E) the image for `<env2>` doesn't exist
//     AND (F) the cache volume for `<env1>` exists
//     AND (G) the cache volume for `<env2>` exists
fn clean_skip_volumes() {
    let test_name = "clean_skip_volumes";
    let root_test_dir = test_setup::assert_create_root_dir(test_name);
    // (1) (2)
    let setup = create_templates_dir(&root_test_dir, test_name);
    assert_noop_run(&root_test_dir, &setup.env1);
    // (3)
    docker::assert_image_exists(&setup.env1_img_tagged_name);
    // (4)
    docker::assert_volume_exists(&setup.env1_cache_vol);
    assert_noop_run(&root_test_dir, &setup.env2);
    // (5)
    docker::assert_image_exists(&setup.env2_img_tagged_name);
    // (6)
    docker::assert_volume_exists(&setup.env2_cache_vol);

    let cmd_result =
        run_test_cmd(&root_test_dir, &["clean", "--skip-volumes"]);

    cmd_result
        // (A)
        .code(0)
        // (B)
        .stderr("")
        // (C)
        .stdout("");
    // (D)
    docker::assert_image_doesnt_exist(&setup.env1_img_tagged_name);
    // (E)
    docker::assert_image_doesnt_exist(&setup.env2_img_tagged_name);
    // (F)
    docker::assert_volume_exists(&setup.env1_cache_vol);
    // (G)
    docker::assert_volume_exists(&setup.env2_cache_vol);
}

#[test]
// Given (1) the dock file defines an environment called `<env1>`
//     AND (2) `<env1>` defines a cache volume
//     AND (3) the image for `<env1>` exists
//     AND (4) the cache volume for `<env1>` exists
//     AND (5) a container for the image for `<env1>` exists
// When `clean` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT is empty
//     AND (D) the image for `<env1>` exists
//     AND (E) the cache volume for `<env1>` doesn't exist
fn clean_image_in_use() {
    let test_name = "clean_image_in_use";
    let root_test_dir = test_setup::assert_create_root_dir(test_name);
    // (1) (2)
    let setup = create_templates_dir(&root_test_dir, test_name);
    assert_noop_run(&root_test_dir, &setup.env1);
    // (3)
    docker::assert_image_exists(&setup.env1_img_tagged_name);
    // (4)
    docker::assert_volume_exists(&setup.env1_cache_vol);
    // (5)
    assert_run::assert_run("docker", &["create", &setup.env1_img_tagged_name]);

    let cmd_result = run_test_cmd(&root_test_dir, &["clean"]);

    cmd_result
        // (A)
        .code(0)
        // TODO STDERR should contain a message to indicate that the image
        // couldn't be removed.
        // (B)
        .stderr("")
        // (C)
        .stdout("");
    // (D)
    docker::assert_image_exists(&setup.env1_img_tagged_name);
    // (E)
    docker::assert_volume_doesnt_exist(&setup.env1_cache_vol);
}

#[test]
// Given (1) the dock file defines an environment called `<env1>`
//     AND (2) `<env1>` defines a cache volume
//     AND (3) the image for `<env1>` exists
//     AND (4) the cache volume for `<env1>` exists
//     AND (5) a container for the image for `<env1>` exists
//     AND (6) the container uses the cache volume for `<env1>`
// When `clean` is run
// Then (A) the command is successful
//     AND (B) the command STDERR is empty
//     AND (C) the command STDOUT is empty
//     AND (D) the image for `<env1>` exists
//     AND (E) the cache volume for `<env1>` doesn't exist
fn clean_volume_in_use() {
    let test_name = "clean_volume_in_use";
    let root_test_dir = test_setup::assert_create_root_dir(test_name);
    // (1) (2)
    let setup = create_templates_dir(&root_test_dir, test_name);
    assert_noop_run(&root_test_dir, &setup.env1);
    // (3)
    docker::assert_image_exists(&setup.env1_img_tagged_name);
    // (4)
    docker::assert_volume_exists(&setup.env1_cache_vol);
    // (5) (6)
    assert_run::assert_run(
        "docker",
        &[
            "create",
            &format!("--volume={}:/test", setup.env1_cache_vol),
            &setup.env1_img_tagged_name,
        ],
    );

    let cmd_result = run_test_cmd(&root_test_dir, &["clean"]);

    cmd_result
        // (A)
        .code(0)
        // TODO STDERR should contain a message to indicate that the image
        // couldn't be removed.
        // (B)
        .stderr("")
        // (C)
        .stdout("");
    // (D)
    docker::assert_image_exists(&setup.env1_img_tagged_name);
    // (E)
    docker::assert_volume_exists(&setup.env1_cache_vol);
}
