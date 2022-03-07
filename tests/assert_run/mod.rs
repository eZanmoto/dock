// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::str;

use crate::assert_cmd::Command as AssertCommand;

pub fn assert_run_stdout(prog: &str, args: &[&str]) -> String {
    let mut cmd = AssertCommand::new(prog);
    cmd.args(args);
    cmd.env_clear();

    let result = cmd.assert().code(0);
    let stdout = &result.get_output().stdout;

    str::from_utf8(&stdout)
        .expect("couldn't decode STDOUT as UTF-8")
        .to_string()
}
