// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::fs::File;
use std::io::Error as IoError;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;

use snafu::ResultExt;
use snafu::Snafu;

// `find_and_open_file` reads the file named `file_name` in `start` or the
// deepest of `start`s ancestor directories that contains a file named
// `file_name`.
pub fn find_and_open_file(start: &Path, file_name: &str)
    -> Result<Option<(PathBuf, File)>, FindAndOpenFileError>
{
    let mut cur_dir = start.to_path_buf();
    loop {
        let path = cur_dir.clone().join(file_name);

        let maybe_conts = try_open(&path)
            .context(ReadFailed{path})?;

        if let Some(conts) = maybe_conts {
            return Ok(Some((cur_dir, conts)));
        }

        if !cur_dir.pop() {
            return Ok(None);
        }
    }
}

#[derive(Debug, Snafu)]
pub enum FindAndOpenFileError {
    ReadFailed{source: IoError, path: PathBuf},
}

// `try_open` returns `path` opened in read-only mode, or `None` if it doesn't
// exist, or an error if one occurred.
pub fn try_open<P: AsRef<Path>>(path: P) -> Result<Option<File>, IoError> {
    match File::open(path) {
        Ok(conts) => {
            Ok(Some(conts))
        },
        Err(err) => {
            if err.kind() == ErrorKind::NotFound {
                Ok(None)
            } else {
                Err(err)
            }
        },
    }
}
