// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::os::unix::ffi::OsStrExt;
use std::io::Error as IoError;
use std::io::Write;

use crate::logging_process::CmdLoggerMsg;
use crate::logging_process::CommandLogger;

#[derive(Debug)]
pub enum Stream {
    Stdout,
    Stderr,
}

pub struct CapturingCmdLogger {
    pub chunks: Vec<(Stream, Vec<u8>)>,
}

impl CapturingCmdLogger {
    pub fn new() -> Self {
        CapturingCmdLogger{chunks: vec![]}
    }
}

impl CommandLogger for CapturingCmdLogger {
    fn log(&mut self, msg: CmdLoggerMsg) {
        match msg {
            CmdLoggerMsg::Cmd(_) => {
            },
            CmdLoggerMsg::StdoutWrite(bs) => {
                self.chunks.push((Stream::Stdout, bs.to_vec()));
            },
            CmdLoggerMsg::StderrWrite(bs) => {
                self.chunks.push((Stream::Stderr, bs.to_vec()));
            },
        };
    }
}

pub struct PrefixingCmdLogger<'a> {
    w: &'a mut dyn Write,
    cmd_prefix: &'a [u8],
    stdout_prefixer: Prefixer<'a>,
    stderr_prefixer: Prefixer<'a>,
    pub err: Option<IoError>,
}

impl<'a> PrefixingCmdLogger<'a> {
    pub fn new(
        w: &'a mut dyn Write,
        cmd_prefix: &'a [u8],
        stdout_prefixer: Prefixer<'a>,
        stderr_prefixer: Prefixer<'a>,
    ) -> Self {
        PrefixingCmdLogger{
            w,
            cmd_prefix,
            stdout_prefixer,
            stderr_prefixer,
            err: None,
        }
    }

    fn try_log(&mut self, msg: &CmdLoggerMsg) -> Result<(), IoError> {
        // TODO Perform line buffering.
        match msg {
            CmdLoggerMsg::Cmd(cmd_line) => {
                self.w.write_all(self.cmd_prefix)?;

                for s in *cmd_line {
                    self.w.write_all((*s).as_bytes())?;
                    self.w.write_all(&[SPACE])?;
                }

                self.w.write_all(&[NEWLINE])?;
            },
            CmdLoggerMsg::StdoutWrite(bs) => {
                self.w.write_all(&self.stdout_prefixer.prefix(bs))?;
            },
            CmdLoggerMsg::StderrWrite(bs) => {
                self.w.write_all(&self.stderr_prefixer.prefix(bs))?;
            },
        }

        self.w.flush()?;

        Ok(())
    }
}

impl<'a> CommandLogger for PrefixingCmdLogger<'a> {
    fn log(&mut self, msg: CmdLoggerMsg) {
        if self.err.is_some() {
            return;
        }

        if let Err(e) = self.try_log(&msg) {
            self.err = Some(e);
        }
    }
}

pub struct Prefixer<'a> {
    prefix: &'a [u8],
    due_prefix: bool,
}

impl<'a> Prefixer<'a> {
    pub fn new(prefix: &'a [u8]) -> Self {
        Prefixer{prefix, due_prefix: true}
    }

    // TODO This is likely to inefficient due to the creation of new values
    // instead of borrowing.
    pub fn prefix(&mut self, buf: &[u8]) -> Vec<u8> {
        if buf.is_empty() {
            return vec![];
        }

        let mut prefixed_buf = vec![];

        let mut first = true;
        for bs in buf.split_inclusive(|b| is_newline(*b)) {
            if !first || self.due_prefix {
                prefixed_buf.extend(self.prefix);
            }
            first = false;

            prefixed_buf.extend(bs);
        }

        let last = &buf[buf.len() - 1];
        self.due_prefix = is_newline(*last);

        prefixed_buf
    }
}

fn is_newline(b: u8) -> bool {
    b == NEWLINE
}

const SPACE: u8 = 0x20;
const NEWLINE: u8 = 0x0a;

pub struct StdCmdLogger<'a> {
    stdout: &'a mut dyn Write,
    stderr: &'a mut dyn Write,
    pub err: Option<IoError>,
}

impl<'a> StdCmdLogger<'a> {
    pub fn new(
        stdout: &'a mut dyn Write,
        stderr: &'a mut dyn Write,
    ) -> Self {
        StdCmdLogger{stdout, stderr, err: None}
    }

    fn try_log(&mut self, msg: &CmdLoggerMsg) -> Result<(), IoError> {
        match msg {
            CmdLoggerMsg::Cmd(_) => {
            },
            CmdLoggerMsg::StdoutWrite(bs) => {
                self.stdout.write_all(bs)?;
                self.stdout.flush()?;
            },
            CmdLoggerMsg::StderrWrite(bs) => {
                self.stderr.write_all(bs)?;
                self.stderr.flush()?;
            },
        }

        Ok(())
    }
}

impl<'a> CommandLogger for StdCmdLogger<'a> {
    fn log(&mut self, msg: CmdLoggerMsg) {
        if self.err.is_some() {
            return;
        }

        if let Err(err) = self.try_log(&msg) {
            self.err = Some(err);
        }
    }
}
