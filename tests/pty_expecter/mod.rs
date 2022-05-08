// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::os::unix::io::FromRawFd;
use std::process::Command;
use std::process::Stdio;
use std::str;
use std::string::ToString;

use crate::nix::pty;
use crate::nix::pty::OpenptyResult;
use crate::nix::sys::time::TimeVal;
use crate::nix::sys::time::TimeValLike;

use crate::timeout::Error as TimeoutError;
use crate::timeout::FdReadWriter;

#[test]
// TODO Refactor this test into BDD-comment tests.
fn sequence() {
    unsafe {
        PtyExpecter::with_new(
            TimeVal::seconds(3),
            "/bin/sh",
            &[],
            |mut pty| -> Result<(), ()> {
                pty.expect("$ ");

                // We add quote marks around `hi` so that `expect("hi\r")`
                // doesn't match the characters that the terminal itself
                // echoes. TODO Consider disabling terminal echo in order to
                // simplify expectations.
                pty.send("echo 'hi'\n");

                pty.expect("hi\r");

                pty.expect("$ ");

                pty.send("exit\n");

                pty.expect_eof();

                Ok(())
            },
        )
            .expect("PTY expectation failed");
    }
}

struct PtyExpecter {
    stream: FdReadWriter,
    timeout: TimeVal,
    buf: Vec<u8>,
    buf_used: usize,
    last_match: usize,
}

impl PtyExpecter {
    unsafe fn with_new<F, T, E>(
        timeout: TimeVal,
        prog: &str,
        args: &[&str], f: F,
    )
        -> Result<T, E>
    where
        F: FnOnce(Self) -> Result<T, E>
    {
        let OpenptyResult{master: controller_fd, slave: follower_fd} =
            pty::openpty(None, None)
                .expect("couldn't open a new PTY");

        let new_follower_stdio = || Stdio::from_raw_fd(follower_fd);

        let mut child =
            Command::new(prog)
                .args(args)
                .stdin(new_follower_stdio())
                .stdout(new_follower_stdio())
                .stderr(new_follower_stdio())
                .spawn()
                .expect("couldn't spawn the new PTY process");

        let expecter = Self{
            stream: FdReadWriter::from_raw_fd(controller_fd),
            timeout,
            // TODO Consider taking the capacity as a parameter instead.
            buf: Vec::with_capacity(BUF_MIN_SPACE),
            buf_used: 0,
            last_match: 0,
        };

        let result = f(expecter);

        child.kill()
            .expect("couldn't kill the PTY process");

        child.wait()
            .expect("couldn't wait for the PTY process");

        // NOTE We don't close the file descriptors for the PTY opened at the
        // start of the function because their ownership is consumed by
        // different objects that automatically close the descriptors when the
        // objects go out of scope. See the contract of `from_raw_fd()` in
        // <https://doc.rust-lang.org/std/os/unix/io/trait.FromRawFd.html> for
        // more information.

        result
    }

    fn send(&mut self, substr: &str) {
        let seq = substr.as_bytes();
        let mut cursor = 0;

        while cursor < seq.len() {
            let subseq = &seq[cursor..];

            let n = self.stream.write(subseq, Some(self.timeout))
                .unwrap_or_else(|_| self.fail(&format!(
                    "couldn't write to stream; sending '{}'",
                    substr,
                )))
                .unwrap_or_else(|| self.fail(&format!(
                    "write timed out; sending '{}'",
                    substr,
                )));

            assert!(n != 0, "stream didn't accept any bytes");

            cursor += n;
        }
    }

    fn expect(&mut self, substr: &str) {
        let seq = substr.as_bytes();

        loop {
            // NOTE It's important that we attempt to match before attempting
            // to read again because there may already be a match in the
            // currently unmatched portion of the buffer.
            if let Some(i) = self.matches(seq) {
                self.last_match = i;
                break;
            }

            let n = self.read_next(self.buf_used)
                .unwrap_or_else(|_| self.fail(&format!(
                    "couldn't read from stream; expecting '{}'",
                    substr,
                )))
                .unwrap_or_else(|| self.fail(&format!(
                    "read timed out; expecting '{}'",
                    substr,
                )));

            assert!(n != 0, "unexpected EOF");

            self.buf_used += n;
        }
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

    fn read_next(&mut self, cursor: usize)
        -> Result<Option<usize>, TimeoutError>
    {
        self.resize_buf_if_needed();

        let buf_scratch = &mut self.buf[cursor..];

        let result = self.stream.read(buf_scratch, Some(self.timeout));

        // If we encounter an error code 5, which corresponds to "Input/output
        // error" in Rust, we regard that as an EOF signal and convert it to
        // match the Rust convention for `read`. This information is supported
        // by a number of non-authoritative sources, but a good summary is
        // provided by <https://unix.stackexchange.com/a/538271>:
        //
        // > On Linux, a `read()` on the master side of a pseudo-tty will
        // > return `-1` and set `ERRNO` to `EIO` when all the handles to its
        // > slave side have been closed, but will either block or return
        // > `EAGAIN` before the slave has been first opened.
        //
        // It is presumed that `EIO` corresponds to "Input/output error".
        if let Err(TimeoutError::OperationFailed{ref source}) = result {
            if source.raw_os_error() == Some(5) {
                return Ok(Some(0));
            }
        }

        result
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

    fn expect_eof(&mut self) {
        loop {
            let n = self.read_next(0)
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
