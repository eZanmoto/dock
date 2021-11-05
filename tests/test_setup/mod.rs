// Copyright 2021 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::collections::HashMap;
use std::fs;
use std::panic;

const IMAGE_NAME_ROOT: &str = "ezanmoto/dock.test";

pub fn create(
    test_name: &str,
    fs_state: &HashMap<&str, &str>,
)
    -> TestSetup
{
    let test_dir = assert_create_root_dir(test_name);
    assert_write_fs_state(&test_dir, fs_state);

    let image_tagged_name =
        format!("{}.{}:latest", IMAGE_NAME_ROOT, test_name);

    TestSetup{
        dir: test_dir,
        image_tagged_name,
    }
}

pub struct TestSetup {
    pub dir: String,
    pub image_tagged_name: String,
}

pub fn assert_create_root_dir(name: &str) -> String {
    assert_create_dir(env!("TEST_DIR").to_string(), name)
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
