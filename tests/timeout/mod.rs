// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::env;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Error as IoError;
use std::io::Read;
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::FromRawFd;
use std::os::unix::io::RawFd;
use std::str;

use crate::nix::errno::Errno;
use crate::nix::sys::select;
use crate::nix::sys::select::FdSet;
use crate::nix::sys::time::TimeVal;
use crate::nix::sys::time::TimeValLike;
use crate::nix::unistd;

use crate::snafu::ResultExt;
use crate::snafu::Snafu;

/// `FdReadWriter` is a readable and writeable interface to a file descriptor
/// where a timeout parameter can be provided for these operations.
struct FdReadWriter {
    // `fds` only contains the single file descriptor for this object.
    fds: FdSet,
    file: File,
}

impl FdReadWriter {
    fn new(fd: RawFd) -> Self {
        let mut fds = FdSet::new();
        fds.insert(fd);

        // NOTE According to the documentation for the `FromRawFd` trait:
        //
        // > This function consumes ownership of the specified file descriptor.
        // > The returned object will take responsibility for closing it when
        // > the object goes out of scope.
        //
        // Note that we retain access to the underlying `fd` through the `fds`
        // field, which we use with `select` to determine when `fd` is ready
        // for reading/writing. This violates the ownership contract described
        // above and so may result in unexpected behaviour if not used
        // carefully.
        let file = unsafe { File::from_raw_fd(fd) };

        Self{fds, file}
    }

    fn read(&mut self, buf: &mut [u8], timeout: Option<TimeVal>)
        -> Result<Option<usize>, Error>
    {
        let mut read_fds = self.fds;
        let mut t = timeout;

        // TODO We would ideally provide `error_fds` in order to check for
        // error conditions on `self.fds`, but tests to trigger this behaviour
        // were not discovered so far during development.
        let num_fds = select::select(None, &mut read_fds, None, None, &mut t)
            .context(SelectFailed)?;

        if num_fds == 0 {
            // According to the documentation for `select(2)`, which is
            // referenced by
            // <https://docs.rs/nix/0.24.1/nix/sys/select/fn.select.html>:
            //
            // > If the timeout interval expires without the specified
            // > condition being true for any of the specified file
            // > descriptors, the objects pointed to by the `readfds`,
            // > `writefds`, and `errorfds` arguments shall have all bits set
            // > to 0.
            return Ok(None);
        }

        let num_bytes = self.file.read(buf)
            // TODO Investigate whether this failure can occur.
            .context(OperationFailed)?;

        Ok(Some(num_bytes))
    }
}

#[derive(Debug, Snafu)]
pub enum Error {
    SelectFailed{source: Errno},
    OperationFailed{source: IoError},
}

#[cfg(test)]
mod tests {
    // Some of these tests force the behaviour of `read` to be deterministic.
    // Often, the number of bytes that `read` populates is non-deterministic,
    // but this can be made deterministic by using a buffer with a length of 1,
    // or by ensuring there's only one byte left in the stream. In these
    // situations `read` must populate exactly one byte - populating 0 bytes
    // isn't supported by the `Read` trait in this case, because this is used
    // to signal EOF.

    use super::*;

    #[test]
    // Given (1) a file containing a single byte
    //     AND (2) the file is open for reading
    //     AND (3) a new `FdReadWriter` created from the file descriptor
    //     AND (4) a non-empty buffer `buf`
    // When the `FdReadWriter` is read to `buf`
    // Then (A) the result is `Ok(Some(1))`
    //     AND (B) `buf` contains the byte from the file
    fn read() {
        let data = "A";
        // (1)
        let test_file_path = assert_write_test_file("read", data.as_bytes());
        // (2)
        let f = File::open(test_file_path)
            .expect("couldn't open test file for reading");
        // (3)
        let mut stream = FdReadWriter::new(f.as_raw_fd());
        // (4)
        let mut buf = [0; 0x100];

        let result = stream.read(&mut buf, None);

        // (A)
        let n = result
            .expect("result wasn't `Ok`")
            .expect("result wasn't `Some`");
        assert_eq!(1, n);
        // (B)
        assert_eq!(Ok(data), str::from_utf8(&buf[..1]));
    }

    fn assert_write_test_file(test_name: &str, content: &[u8]) -> String {
        let path = format!("{}/{}", TEST_DIR, test_name);

        // TODO Create a new sub-test directory to contain test files.
        let mut f =
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .expect("couldn't open test file for writing");

        f.write_all(content)
            .expect("couldn't write test file");

        path
    }

    // TODO Duplicated from `tests/test_setup/mod.rs`.
    const TEST_DIR: &str = env!("TEST_DIR");

    #[test]
    // Given (1) a file containing two bytes
    //     AND (2) the file is open for reading
    //     AND (3) a new `FdReadWriter` created from the file descriptor
    //     AND (4) a buffer `buf` of length 1
    //     AND (5) the `FdReadWriter` was read into `buf` once
    // When the `FdReadWriter` is read to `buf`
    // Then (A) the result is `Ok(Some(1))`
    //     AND (B) `buf` contains the second byte from the file
    fn read_twice() {
        let test_name = "read_twice";
        let data = "AB";
        // (1)
        let test_file_path =
            assert_write_test_file(test_name, data.as_bytes());
        // (2)
        let f = File::open(test_file_path)
            .expect("couldn't open test file for reading");
        // (3)
        let mut stream = FdReadWriter::new(f.as_raw_fd());
        // (4)
        let mut buf = [0; 1];
        // (5)
        let result = stream.read(&mut buf, None);
        assert_matches!(result, Ok(Some(1)));

        let result = stream.read(&mut buf, None);

        // (A)
        let n = result
            .expect("result wasn't `Ok`")
            .expect("result wasn't `Some`");
        assert_eq!(1, n);
        // (B)
        assert_eq!(Ok(&data[1..2]), str::from_utf8(&buf[..1]));
    }

    #[test]
    // Given (1) a file containing a single byte
    //     AND (2) the file is open for reading
    //     AND (3) a new `FdReadWriter` created from the file descriptor
    //     AND (4) the `FdReadWriter` was read once
    // When the `FdReadWriter` is read to a non-empty buffer
    // Then (A) the result is `Ok(Some(0))`
    fn read_until_eof() {
        let test_name = "read_until_eof";
        // (1)
        let test_file_path = assert_write_test_file(test_name, "A".as_bytes());
        // (2)
        let f = File::open(test_file_path)
            .expect("couldn't open test file for reading");
        // (3)
        let mut stream = FdReadWriter::new(f.as_raw_fd());
        let mut buf = [0; 0x100];
        // (4)
        let result = stream.read(&mut buf, None);
        assert_matches!(result, Ok(Some(1)));

        let result = stream.read(&mut buf, None);

        // (A)
        assert_matches!(result, Ok(Some(0)));
    }

    #[test]
    // Given (1) a pipe
    //     AND (2) nothing has been written to the pipe
    //     AND (3) the target of the pipe has been closed
    //     AND (4) a new `FdReadWriter` created from the source of the pipe
    // When the `FdReadWriter` is read from
    // Then (A) the result is `Ok(Some(0))`
    fn eof() {
        let (src, tgt) = unistd::pipe()
            .expect("couldn't create pipe");
        unistd::close(tgt)
            .expect("couldn't close target end of pipe");
        let mut stream = FdReadWriter::new(src);

        let result = stream.read(&mut [0; 0x100], None);

        // (A)
        assert_matches!(result, Ok(Some(0)));
        // We don't close the source end of the pipe because EOF has already
        // been read from it; attempting to close the pipe at this time during
        // testing has resulted in an `EBADF` from `unistd::close(src)`.
    }

    #[test]
    // Given (1) a pipe
    //     AND (2) nothing has been written to the pipe
    //     AND (3) the pipe hasn't been closed
    //     AND (4) a new `FdReadWriter` created from the pipe
    //     AND (5) a timeout of 3 seconds
    // When the `FdReadWriter` is read from
    // Then (A) the result is `Ok(None)`
    fn read_timeout() {
        // (1) (2) (3)
        let (src, tgt) = unistd::pipe()
            .expect("couldn't create pipe");
        // (4)
        let mut stream = FdReadWriter::new(src);
        // (5)
        let timeout = TimeVal::seconds(3);

        let result = stream.read(&mut [0; 0x100], Some(timeout));

        // (A)
        assert_matches!(result, Ok(None));
        unistd::close(tgt)
            .expect("couldn't close target end of pipe");
        unistd::close(src)
            .expect("couldn't close source end of pipe");
    }

    #[test]
    // Given (1) a file descriptor for a non-empty stream
    //     AND (2) a new `FdReadWriter` created from the file descriptor
    // When the `FdReadWriter` is read to an empty buffer
    // Then (A) the result is `Ok(Some(0))`
    fn read_to_empty_buffer() {
        let test_name = "read_to_empty_buffer";
        let test_file_path = assert_write_test_file(test_name, "A".as_bytes());
        // (1)
        let f = File::open(test_file_path)
            .expect("couldn't open test file for reading");
        // (2)
        let mut stream = FdReadWriter::new(f.as_raw_fd());

        let result = stream.read(&mut [0; 0], None);

        // (A)
        assert_matches!(result, Ok(Some(0)));
    }

    #[test]
    // Given (1) a pipe
    //     AND (2) the source of the pipe has been closed
    //     AND (3) a new `FdReadWriter` created from the pipe
    // When the `FdReadWriter` is read to a non-empty buffer
    // Then (A) the result is an `Err`
    //     AND (B) the root error is `Errno::EINVAL`
    fn read_from_closed_fd_fails() {
        // (1)
        let (src, tgt) = unistd::pipe()
            .expect("couldn't create pipe");
        // (2)
        unistd::close(src)
            .expect("couldn't close source end of pipe");
        // (3)
        let mut stream = FdReadWriter::new(src);

        let r = stream.read(&mut [0; 0x100], None);

        // (A) (B)
        assert_matches!(r, Err(Error::SelectFailed{source: Errno::EBADF}));
        unistd::close(tgt)
            .expect("couldn't close target end of pipe");
    }

    #[test]
    // Given (1) a valid file descriptor
    //     AND (2) a new `FdReadWriter` created from the file descriptor
    //     AND (3) a negative timeout
    // When the `FdReadWriter` is read to a non-empty buffer
    // Then (A) the result is an `Err`
    //     AND (B) the root error is `Errno::EINVAL`
    fn read_with_negative_timeout_fails() {
        let test_name = "read_with_negative_timeout_fails";
        let test_file_path = assert_write_test_file(test_name, "A".as_bytes());
        let f = File::open(test_file_path)
            .expect("couldn't open test file for reading");
        // (1) (2)
        let mut stream = FdReadWriter::new(f.as_raw_fd());
        // (3)
        let timeout = TimeVal::seconds(-1);

        let r = stream.read(&mut [0; 0x100], Some(timeout));

        // (A) (B)
        assert_matches!(r, Err(Error::SelectFailed{source: Errno::EINVAL}));
    }
}
