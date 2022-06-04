// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use crate::docker;
use crate::pty::expecter::Expecter;
use crate::test_setup;
use crate::test_setup::Definition;

use crate::nix::sys::time::TimeVal;
use crate::nix::sys::time::TimeValLike;

#[test]
fn shell_without_bash_fails() {
    let test_name = "shell_without_bash_fails";
    let test = test_setup::assert_apply_with_dock_yaml(
        indoc!{"
            shell: /bin/bash
        "},
        &Definition{
            name: test_name,
            dockerfile_steps: "",
            fs: &hashmap!{},
        },
    );
    let mut pty = unsafe { Expecter::new(
        "/app/target/debug/dock",
        &[],
        TimeVal::seconds(10),
        &test.dir,
    ) };

    defer!{
        docker::assert_kill_image_container(&test.image_tagged_name);
    };

    pty.expect("exec /bin/bash failed: No such file or directory");

    pty.expect_eof();
}
