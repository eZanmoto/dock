// Copyright 2022-2023 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::str;
use std::ffi::OsStr;
use std::string::ToString;

use crate::nix::sys::time::TimeVal;
use crate::nix::sys::time::TimeValLike;
use serial_test::serial;

use crate::timeout::Error as TimeoutError;
use super::Pty;

// TODO Refactor tests in this file into BDD-comment tests.

// TODO Tests that create shells in this module are marked as `serial`. This is
// because when they run in parallel, it often happens that the `cargo test`
// job is stopped in the shell that it's run under, and `fg` must be used to
// bring it to the foreground to allow it to complete. This behaviour should be
// investigated and ideally rectified when time allows.

#[test]
#[serial]
fn shell_session_is_interactive() {
    let timeout = TimeVal::seconds(3);
    let sh = OsStr::new("/bin/sh");
    let mut pty = unsafe { Expecter::new(sh, &[], timeout, "/") };

    pty.expect("$ ");

    // We add quote marks around `hi` so that `expect("hi\r")` doesn't match
    // the characters that the terminal itself echoes. TODO Consider disabling
    // terminal echo in order to simplify expectations.
    pty.send("echo 'hi'\n");

    pty.expect("hi\r");

    pty.expect("$ ");

    pty.send("exit\n");

    pty.expect_eof();
}

#[test]
#[serial]
fn pty_without_initial_size_gets_expected_defaults() {
    let timeout = TimeVal::seconds(3);
    let sh = OsStr::new("/bin/sh");
    let mut pty = unsafe { Expecter::new(sh, &[], timeout, "/") };

    pty.expect("$ ");
    pty.send("tput cols\n");
    pty.expect("80\r");
    pty.expect("$ ");
    pty.send("tput lines\n");
    pty.expect("24\r");
    pty.expect("$ ");
    pty.send("exit\n");
    pty.expect_eof();
}

#[test]
fn pty_is_created_with_correct_width() {
    let timeout = TimeVal::seconds(3);
    let sh = OsStr::new("tput");
    let mut pty = unsafe {
        Expecter::new_with_winsize(sh, &["cols"], timeout, "/", (50, 10))
    };

    pty.expect("10");
    pty.expect_eof();
}

#[test]
fn pty_is_created_with_correct_height() {
    let timeout = TimeVal::seconds(3);
    let sh = OsStr::new("tput");
    let mut pty = unsafe {
        Expecter::new_with_winsize(sh, &["lines"], timeout, "/", (10, 50))
    };

    pty.expect("10");
    pty.expect_eof();
}

#[test]
#[serial]
fn shell_in_pty_gets_initial_terminal_size() {
    let timeout = TimeVal::seconds(3);
    let sh = OsStr::new("/bin/sh");
    let mut pty = unsafe {
        Expecter::new_with_winsize(sh, &[], timeout, "/", (10, 20))
    };

    pty.expect("$ ");
    pty.send("tput cols\n");
    pty.expect("20\r");
    pty.expect("$ ");
    pty.send("tput lines\n");
    pty.expect("10\r");
    pty.expect("$ ");
    pty.send("exit\n");
    pty.expect_eof();
}

#[test]
#[serial]
fn set_winsize_updates_pty_size() {
    let timeout = TimeVal::seconds(3);
    let sh = OsStr::new("/bin/sh");
    let mut pty = unsafe { Expecter::new(sh, &[], timeout, "/") };

    pty.expect("$ ");
    pty.send("tput cols\n");
    pty.expect("80\r");
    pty.expect("$ ");
    pty.send("tput lines\n");
    pty.expect("24\r");
    pty.expect("$ ");

    pty.set_winsize(10, 20);

    pty.expect("$ ");
    pty.send("tput cols\n");
    pty.expect("20\r");
    pty.expect("$ ");
    pty.send("tput lines\n");
    pty.expect("10\r");

    pty.expect("$ ");
    pty.send("exit\n");
    pty.expect_eof();
}

pub struct Expecter {
    pty: Pty,
    timeout: TimeVal,
    buf: Vec<u8>,
    buf_used: usize,
    last_match: usize,
}

impl Expecter {
    pub unsafe fn new(
        prog: &OsStr,
        args: &[&str],
        timeout: TimeVal,
        current_dir: &str,
    ) -> Self {
        Self{
            pty: Pty::new(prog, args, current_dir),
            timeout,
            // TODO Consider taking the capacity as a parameter instead.
            buf: Vec::with_capacity(BUF_MIN_SPACE),
            buf_used: 0,
            last_match: 0,
        }
    }

    pub unsafe fn new_with_winsize(
        prog: &OsStr,
        args: &[&str],
        timeout: TimeVal,
        current_dir: &str,
        size: (u16, u16),
    ) -> Self {
        Self{
            pty: Pty::new_with_winsize(prog, args, current_dir, size),
            timeout,
            // TODO Consider taking the capacity as a parameter instead.
            buf: Vec::with_capacity(BUF_MIN_SPACE),
            buf_used: 0,
            last_match: 0,
        }
    }

    pub fn set_winsize(&mut self, rows: u16, cols: u16) {
        self.pty.set_winsize(rows, cols);
    }

    pub fn send(&mut self, substr: &str) {
        let seq = substr.as_bytes();
        let mut cursor = 0;

        while cursor < seq.len() {
            let subseq = &seq[cursor..];

            let n = self.pty.write(subseq, Some(self.timeout))
                .unwrap_or_else(|_| self.fail(&format!(
                    "couldn't write to PTY; sending '{}'",
                    substr,
                )))
                .unwrap_or_else(|| self.fail(&format!(
                    "write timed out; sending '{}'",
                    substr,
                )));

            if n == 0 {
                self.fail(&format!(
                    "PTY didn't accept bytes; sending '{}'",
                    substr,
                ));
            }

            cursor += n;
        }
    }

    pub fn expect(&mut self, substr: &str) {
        let seq = substr.as_bytes();

        loop {
            // NOTE It's important that we attempt to match before attempting
            // to read again because there may already be a match in the
            // currently unmatched portion of the buffer.
            if let Some(i) = self.matches(seq) {
                self.last_match = i + seq.len();
                break;
            }

            self.resize_buf_if_needed();

            let n = self.read_to_buf_from(self.buf_used)
                .unwrap_or_else(|_| self.fail(&format!(
                    "couldn't read from PTY; expecting '{}'",
                    substr,
                )))
                .unwrap_or_else(|| self.fail(&format!(
                    "read timed out; expecting '{}'",
                    substr,
                )));

            if n == 0 {
                self.fail(&format!("unexpected EOF; expecting '{substr}'"));
            }

            self.buf_used += n;
        }
    }

    fn read_to_buf_from(&mut self, cursor: usize)
        -> Result<Option<usize>, TimeoutError>
    {
        self.pty.read(&mut self.buf[cursor..], Some(self.timeout))
    }

    fn fail(&self, msg: &str) -> ! {
        let buf = str::from_utf8(&self.buf[..self.buf_used])
            .expect("couldn't render buffer as `str`");

        // TODO Try to render the buffer cursor position using an "out-of-band"
        // rendering.
        // TODO Highlight whitespace in buffer rendering.
        panic!(
            "\n\n\t{}:\n\n\t> {}<|>{}<\n\n",
            msg,
            Self::render_buffer_lines(&buf[..self.last_match], "\t> "),
            Self::render_buffer_lines(&buf[self.last_match..], "\t> "),
        );
    }

    fn render_buffer_lines(source: &str, sep: &str) -> String {
        let lines: Vec<String> =
            source
                .lines()
                .map(ToString::to_string)
                .collect();

        let mut target = lines.join(&("<\n".to_string() + sep));

        // We check `source` because `source.lines()` drops trailing newlines:
        //
        // > A string that ends with a final line ending will return the same
        // > lines as an otherwise identical string without a final line
        // > ending.
        if source.ends_with('\n') {
            target += "<\n";
            target += sep;
        }

        target
    }

    fn matches(&self, seq: &[u8]) -> Option<usize> {
        let unmatched = &self.buf[self.last_match..];

        // Adapted from <https://stackoverflow.com/a/35907071>. Note that this
        // approach may suffer from poor efficiency in terms of time and space,
        // and so is currently just intended to be used for testing purposes
        // with small data sets. The previous link contains references to
        // approaches that may be more performant.
        unmatched.windows(seq.len())
            .position(|window| window == seq)
    }

    fn resize_buf_if_needed(&mut self) {
        if self.buf.len() - self.buf_used < BUF_MIN_SPACE {
            self.buf.resize(self.buf.len() + BUF_MIN_SPACE, 0);
        }
    }

    pub fn expect_eof(&mut self) {
        loop {
            let n = self.read_to_buf_from(0)
                .unwrap_or_else(|_| self.fail(
                    "couldn't read from stream; expecting EOF",
                ))
                .unwrap_or_else(|| self.fail("read timed out; expecting EOF"));

            if n == 0 {
                break;
            }
        }
    }
}

const BUF_MIN_SPACE: usize = 0x100;
