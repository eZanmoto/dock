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
fn shell_uses_correct_image() {
    let test_name = "shell_uses_correct_image";
    let test = test_setup::assert_apply_with_empty_dock_yaml(&Definition{
        name: test_name,
        dockerfile_steps: &formatdoc!{
            "
                RUN echo '{test_name}' > test.txt
            ",
            test_name = test_name,
        },
        fs: &hashmap!{},
    });
    let mut pty = unsafe { Expecter::new(
        // TODO Abstract the `dock` program being tested.
        "/app/target/debug/dock",
        &["shell", test_name],
        // We use a long timeout to give `dock` time to rebuild the image
        // before the shell starts.
        TimeVal::seconds(10),
        &test.dir,
    ) };

    // We use `defer!` to run cleanup for tests. We generally use higher-order
    // functions to perform such cleanups, but these are less applicable in the
    // case of tests, where test failures are triggered using panics. As such,
    // we opt to use `defer!` so that the cleanup code will run regardless of
    // how the test exited.
    defer!{
        // We kill the container that we expect `dock` to have started. TODO
        // Document why this is needed.
        docker::assert_kill_image_container(&test.image_tagged_name);
    };

    pty.expect("# ");

    pty.send("cat 'test.txt'\n");

    pty.expect(test_name);

    pty.expect("# ");

    pty.send("exit\n");

    pty.expect_eof();
}

#[test]
fn dock_shell_uses_default_shell_env() {
    let test_name = "dock_shell_uses_default_shell_env";
    let test = test_setup::assert_apply_with_empty_dock_yaml(&Definition{
        name: test_name,
        dockerfile_steps: &formatdoc!{
            "
                RUN echo '{test_name}' > test.txt
            ",
            test_name = test_name,
        },
        fs: &hashmap!{},
    });
    let mut pty = unsafe { Expecter::new(
        "/app/target/debug/dock",
        &["shell"],
        TimeVal::seconds(10),
        &test.dir,
    ) };

    defer!{
        docker::assert_kill_image_container(&test.image_tagged_name);
    };

    pty.expect("# ");

    pty.send("cat 'test.txt'\n");

    pty.expect(test_name);

    pty.expect("# ");

    pty.send("exit\n");

    pty.expect_eof();
}
