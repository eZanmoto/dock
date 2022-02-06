// Copyright 2021-2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::collections::HashMap;
use std::fs;
use std::panic;

const TEST_BASE_IMG: &str = env!("TEST_BASE_IMG");
const IMAGE_NAME_ROOT: &str = env!("TEST_IMG_NAMESPACE");
const TEST_DIR: &str = env!("TEST_DIR");

pub fn assert_apply<'a>(defn: Definition<'a>) -> References {
    assert_apply_with_dockerfile_name("Dockerfile", defn)
}

pub fn assert_apply_with_dockerfile_name<'a>(
    dockerfile_name: &str,
    defn: Definition<'a>,
)
    -> References
{
    let mut fs_state = defn.fs.clone();
    let dockerfile = &formatdoc!{
        "
            FROM {base_img}
            {dockerfile_steps}
        ",
        base_img = TEST_BASE_IMG,
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
        fs::write(format!("{}/{}", &root_dir, fname), fconts)
            .expect("couldn't write test file");
    }
}
