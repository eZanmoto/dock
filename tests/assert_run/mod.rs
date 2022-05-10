// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::str;

use crate::assert_cmd::Command as AssertCommand;

pub fn assert_run_stdout(prog: &str, args: &[&str]) -> String {
    assert_run_output(prog, args).stdout
}

pub fn assert_run_stdout_lines(prog: &str, args: &[&str]) -> Vec<String> {
    assert_run_output(prog, args)
        .stdout
        .lines()
        .map(ToOwned::to_owned)
        .collect()
}

pub fn assert_run_output(prog: &str, args: &[&str]) -> Output {
    let mut cmd = AssertCommand::new(prog);
    cmd.args(args);
    cmd.env_clear();

    let result = cmd.assert().code(0);
    let output = &result.get_output();

    Output{
        stdout: str::from_utf8(&output.stdout)
            .expect("couldn't decode STDOUT as UTF-8")
            .to_string(),
        stderr: str::from_utf8(&output.stderr)
            .expect("couldn't decode STDERR as UTF-8")
            .to_string(),
    }
}

#[derive(Debug)]
pub struct Output {
    pub stdout: String,
    pub stderr: String,
}
