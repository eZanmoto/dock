// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::os::unix::io::FromRawFd;
use std::process::Command;
use std::process::Stdio;
use std::str;

use crate::nix::pty;
use crate::nix::pty::OpenptyResult;
use crate::nix::sys::time::TimeVal;

use crate::timeout::Error as TimeoutError;
use crate::timeout::FdReadWriter;

pub struct Pty {
    stream: FdReadWriter,
}

impl Pty {
    pub unsafe fn with_new<F, T, E>(prog: &str, args: &[&str], f: F)
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

        let pty = Self{
            stream: FdReadWriter::from_raw_fd(controller_fd),
        };

        let result = f(pty);

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

    pub fn read(&mut self, buf: &mut [u8], timeout: Option<TimeVal>)
        -> Result<Option<usize>, TimeoutError>
    {
        let result = self.stream.read(buf, timeout);

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

    pub fn write(&mut self, buf: &[u8], timeout: Option<TimeVal>)
        -> Result<Option<usize>, TimeoutError>
    {
        self.stream.write(buf, timeout)
    }
}
