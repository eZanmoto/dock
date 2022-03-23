// Copyright 2021-2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::collections::HashMap;
use std::fs;
use std::panic;
use std::path::Path;

const TEST_BASE_IMG: &str = env!("TEST_BASE_IMG");
const IMAGE_NAME_ROOT: &str = env!("TEST_IMG_NAMESPACE");
const TEST_DIR: &str = env!("TEST_DIR");

pub fn assert_apply_with_empty_dock_yaml(defn: &Definition) -> References {
    assert_apply_with_dock_yaml("{}", defn)
}

pub fn assert_apply_with_dock_yaml(
    env_defn: &str,
    defn: &Definition,
) -> References {
    let mut fs_state = defn.fs.clone();

    let dock_yaml = &formatdoc!{
        "
            organisation: 'ezanmoto'
            project: 'dock.test'

            environments:
                {test_name}:
                    {env_defn}
        ",
        test_name = defn.name,
        env_defn = env_defn,
    };

    let dock_yaml_name = "dock.yaml";
    let dock_yaml_exists = fs_state.contains_key(dock_yaml_name);
    assert!(!dock_yaml_exists, "`defn.fs` contains `{}`", dock_yaml_name);

    fs_state.insert(dock_yaml_name, dock_yaml);

    assert_apply_with_dockerfile_name(
        &format!("{}.Dockerfile", defn.name),
        &Definition{
            name: defn.name,
            dockerfile_steps: defn.dockerfile_steps,
            fs: &fs_state,
        },
    )
}

pub fn assert_apply(defn: &Definition) -> References {
    assert_apply_with_dockerfile_name("Dockerfile", &defn)
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
        "`defn.fs` contains a file with the Dockerfile name ({})",
        dockerfile_name,
    );

    fs_state.insert(dockerfile_name, dockerfile);

    let test_name = defn.name;
    let test_dir = assert_create_root_dir(test_name);
    assert_write_fs_state(&test_dir, &fs_state);

    let image_tagged_name =
        format!("{}.{}:latest", IMAGE_NAME_ROOT, test_name);

    References{
        dir: test_dir,
        image_tagged_name,
    }
}

pub struct Definition<'a> {
    pub name: &'a str,
    pub dockerfile_steps: &'a str,
    pub fs: &'a HashMap<&'a str, &'a str>,
}

pub struct References {
    pub dir: String,
    pub image_tagged_name: String,
}

pub fn assert_create_root_dir(name: &str) -> String {
    assert_create_dir(TEST_DIR.to_string(), name)
}

pub fn assert_create_dir(dir: String, name: &str) -> String {
    let path = dir + "/" + name;

    fs::create_dir(&path)
        .unwrap_or_else(|_| panic!("couldn't create directory: {}", path));

    path
}

fn assert_write_fs_state(root_dir: &str, fs_state: &HashMap<&str, &str>) {
    for (fname, fconts) in fs_state {
        let raw_test_file = &format!("{}/{}", &root_dir, fname);
        let test_file = Path::new(raw_test_file);

        if let Some(dir) = test_file.parent() {
            fs::create_dir_all(dir)
                .unwrap_or_else(|_| panic!(
                    "couldn't create test directory '{}'",
                    dir.display(),
                ));
        }

        fs::write(test_file, fconts)
            .unwrap_or_else(|_| panic!(
                "couldn't write test file '{}'",
                test_file.display(),
            ));
    }
}
