// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use crate::docker;
use crate::pty::expecter::Expecter;
use crate::test_setup;
use crate::test_setup::Definition;
use crate::test_setup::References;

use crate::nix::sys::time::TimeVal;
use crate::nix::sys::time::TimeValLike;

#[test]
fn shell_uses_correct_image() {
    let test_name = "shell_uses_correct_image";
    let args = &["shell", test_name];
    let (test, mut pty) = unsafe { set_up(test_name, args) };

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

unsafe fn set_up(test_name: &str, cmd_args: &[&str])
    -> (References, Expecter)
{
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

    let pty = new_test_cmd(cmd_args, &test.dir);

    (test, pty)
}

unsafe fn new_test_cmd(cmd_args: &[&str], test_dir: &str) -> Expecter {
    Expecter::new(
        test_setup::test_bin().as_os_str(),
        cmd_args,
        // We use a long timeout to give `dock` time to rebuild the image
        // before the shell starts.
        TimeVal::seconds(10),
        test_dir,
    )
}

#[test]
fn dock_shell_uses_default_shell_env() {
    let test_name = "dock_shell_uses_default_shell_env";
    let args = &["shell"];
    let (test, mut pty) = unsafe { set_up(test_name, args) };

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

#[test]
fn dock_runs_shell_by_default() {
    let test_name = "dock_runs_shell_by_default";
    let args = &[];
    let (test, mut pty) = unsafe { set_up(test_name, args) };

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

#[test]
fn shell_debug_flag() {
    let test_name = "shell_debug_flag";
    let args = &["shell", test_name, "--debug"];
    let (test, mut pty) = unsafe { set_up(test_name, args) };

    defer!{
        docker::assert_kill_image_container(&test.image_tagged_name);
    };

    pty.expect(r"[$] docker build ");
    pty.expect(r"[>] Sending build context to Docker ");
    pty.expect(r"[>] Successfully built ");
    pty.expect(r"[$] docker run ");

    pty.expect("[>] / # ");

    pty.send("cat 'test.txt'\n");

    pty.expect(test_name);

    pty.expect("[>] / # ");

    pty.send("exit\n");

    pty.expect_eof();
}

#[test]
fn shell_overrides_entrypoint() {
    let test_name = "shell_overrides_entrypoint";
    let test = test_setup::assert_apply_with_empty_dock_yaml(&Definition{
        name: test_name,
        dockerfile_steps: &formatdoc!{
            "
                RUN echo '{test_name}' > test.txt

                ENTRYPOINT echo
            ",
            test_name = test_name,
        },
        fs: &hashmap!{},
    });
    let mut pty = unsafe { new_test_cmd(&[], &test.dir) };

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

#[test]
fn shell_with_bash() {
    let test_name = "shell_with_bash";
    let test = test_setup::assert_apply_with_dock_yaml(
        indoc!{"
            shell: /bin/bash
        "},
        &Definition{
            name: test_name,
            dockerfile_steps: indoc!{"
                RUN apk update \\
                    && apk add bash
            "},
            fs: &hashmap!{},
        },
    );
    let mut pty = unsafe { new_test_cmd(&[], &test.dir) };

    defer!{
        docker::assert_kill_image_container(&test.image_tagged_name);
    };

    pty.expect("# ");

    pty.send("echo $0\n");

    pty.expect("/bin/bash");

    pty.expect("# ");

    pty.send("exit\n");

    pty.expect_eof();
}
