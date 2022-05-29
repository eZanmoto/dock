// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use crate::logging_process::CmdLoggerMsg;
use crate::logging_process::CommandLogger;

pub struct CapturingCmdLogger {
    pub streams: Vec<(Stream, Vec<u8>)>,
}

#[derive(Debug)]
pub enum Stream {
    Stdout,
    Stderr,
}

impl CapturingCmdLogger {
    pub fn new() -> Self {
        CapturingCmdLogger{streams: vec![]}
    }
}

impl CommandLogger for CapturingCmdLogger {
    fn log(&mut self, msg: CmdLoggerMsg) {
        match msg {
            CmdLoggerMsg::Cmd(_) => {
            },
            CmdLoggerMsg::StdoutWrite(bs) => {
                self.streams.push((Stream::Stdout, bs.to_vec()));
            },
            CmdLoggerMsg::StderrWrite(bs) => {
                self.streams.push((Stream::Stderr, bs.to_vec()));
            },
        };
    }
}
