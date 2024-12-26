// Copyright 2021-2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::collections::HashMap;
use std::fs;
use std::panic;
use std::path::Path;
use std::path::PathBuf;

use crate::assert_cmd::cargo;

pub const IMAGE_NAME_ROOT: &str = env!("TEST_IMG_NAMESPACE");
pub const TEST_BASE_IMG: &str = env!("TEST_BASE_IMG");
const TEST_DIR: &str = env!("TEST_DIR");
const TEST_ORG: &str = env!("TEST_ORG");
const TEST_PROJ: &str = env!("TEST_PROJ");

pub const TEST_PKG: &str = env!("CARGO_PKG_NAME");

pub fn assert_apply_with_empty_dock_yaml(defn: &Definition) -> References {
    assert_apply_with_dock_yaml("{}", defn)
}

pub fn assert_apply_with_dock_yaml(
    env_defn: &str,
    defn: &Definition,
) -> References {
    assert_apply_with_schema_version("0.1", env_defn, defn)
}

pub fn assert_apply_with_schema_version(
    schema_vsn: &str,
    env_defn: &str,
    defn: &Definition,
) -> References {
    let mut fs_state = defn.fs.clone();

    let dock_yaml_name = "dock.yaml";
    let dock_yaml_exists = fs_state.contains_key(dock_yaml_name);
    assert!(!dock_yaml_exists, "`defn.fs` contains `{dock_yaml_name}`");

    let dock_file = render_dock_file(schema_vsn, defn.name, env_defn);
    fs_state.insert(dock_yaml_name, &dock_file);

    assert_apply_with_dockerfile_name(
        &format!("{}.Dockerfile", defn.name),
        &Definition{
            name: defn.name,
            dockerfile_steps: defn.dockerfile_steps,
            fs: &fs_state,
        },
    )
}

pub fn render_dock_file(schema_vsn: &str, env_name: &str, env_defn: &str)
    -> String
{
    let indented_env_defn =
        env_defn
            .lines()
            .collect::<Vec<&str>>()
            .join("\n    ");

    formatdoc!{
        "
            schema_version: '{schema_vsn}'
            organisation: '{test_org}'
            project: '{test_proj}'
            default_shell_env: '{env_name}'

            environments:
              {env_name}:
                {env_defn}
        ",
        schema_vsn = schema_vsn,
        env_name = env_name,
        env_defn = indented_env_defn,
        test_org = TEST_ORG,
        test_proj = TEST_PROJ,
    }
}

pub fn assert_apply(defn: &Definition) -> References {
    assert_apply_with_dockerfile_name("Dockerfile", defn)
}

pub fn assert_apply_with_dockerfile_name(
    dockerfile_name: &str,
    defn: &Definition,
)
    -> References
{
    let mut fs_state = defn.fs.clone();

    // NOTE `RUN echo {test_name}` ensures that the image created for the test
    // is unique. See `tests/cli/README.md` for more information on why this is
    // used.
    let dockerfile = &formatdoc!{
        "
            FROM {base_img}
            RUN echo {test_name}
            {dockerfile_steps}
        ",
        base_img = TEST_BASE_IMG,
        test_name = defn.name,
        dockerfile_steps = defn.dockerfile_steps,
    };

    assert!(
        !fs_state.contains_key(dockerfile_name),
        "`defn.fs` contains file with the Dockerfile name ({dockerfile_name})",
    );

    fs_state.insert(dockerfile_name, dockerfile);

    let test_name = defn.name;
    let test_dir = assert_create_root_dir(test_name);
    assert_write_fs_state(&test_dir, &fs_state);

    References{
        dir: test_dir,
        image_tagged_name: test_image_tagged_name(test_name),
        cache_volume_prefix: cache_volume_prefix(test_name),
    }
}

pub fn test_image_tagged_name(test_name: &str) -> String {
    format!("{IMAGE_NAME_ROOT}.{test_name}:latest")
}

pub fn cache_volume_prefix(test_name: &str) -> String {
    format!("{TEST_ORG}.{TEST_PROJ}.{test_name}.cache")
}

pub struct Definition<'a> {
    pub name: &'a str,
    pub dockerfile_steps: &'a str,
    pub fs: &'a HashMap<&'a str, &'a str>,
}

pub struct References {
    pub dir: String,
    pub image_tagged_name: String,
    pub cache_volume_prefix: String,
}

impl References {
    pub fn cache_volume_name(&self, suffix: &str) -> String {
        format!("{}.{}", self.cache_volume_prefix, suffix)
    }
}

pub fn assert_create_root_dir(name: &str) -> String {
    assert_create_dir(TEST_DIR.to_string(), name)
}

pub fn assert_create_dir(dir: String, name: &str) -> String {
    let path = dir + "/" + name;

    fs::create_dir(&path)
        .unwrap_or_else(|e| panic!("couldn't create directory: {path}: {e}"));

    path
}

pub fn assert_write_fs_state(root_dir: &str, fs_state: &HashMap<&str, &str>) {
    for (fname, fconts) in fs_state {
        let raw_test_file = &format!("{}/{}", &root_dir, fname);
        let test_file = Path::new(raw_test_file);

        if let Some(dir) = test_file.parent() {
            fs::create_dir_all(dir)
                .unwrap_or_else(|e| panic!(
                    "couldn't create test directory '{}': {}",
                    dir.display(),
                    e,
                ));
        }

        fs::write(test_file, fconts)
            .unwrap_or_else(|e| panic!(
                "couldn't write test file '{}': {}",
                test_file.display(),
                e,
            ));
    }
}

pub fn test_bin() -> PathBuf {
    cargo::cargo_bin(TEST_PKG)
}
