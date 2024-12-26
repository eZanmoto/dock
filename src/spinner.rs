// Copyright 2024 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::io;
use std::io::Write;
use std::sync::mpsc;
use std::sync::mpsc::SendError;
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::time::Duration;

use snafu::ResultExt;
use snafu::Snafu;

pub fn spin<F, T>(msg: String, f: F) -> Result<T, SpinError>
where
    F: FnOnce() -> T,
{
    let (sender, receiver) = mpsc::channel();

    let thread = thread::spawn(move || {
        // TODO Retrieve a reference to STDOUT as a parameter.
        let mut stdout = io::stdout();

        let _ = write!(&mut stdout, "{msg}  ");

        let _ = stdout.flush();

        let mut i = 0;

        // We exit the loop if the receiver receives data, or if the sender
        // disconnects.
        while let Err(TryRecvError::Empty) = receiver.try_recv() {
            let _ =
                write!(&mut stdout, "{}{}", BACKSPACE, CYCLE_CHARS[i]);

            let _ = stdout.flush();

            i = (i + 1) % CYCLE_CHARS.len();

            thread::sleep(Duration::from_millis(100));
        }

        let _ = write!(&mut stdout, "{RESET_CURSOR}{CLEAR_LINE}");

        let _ = stdout.flush();
    });

    let result = f();

    sender.send(())
        .context(SendEndSignalFailed)?;

    // The error returned by `thread.join()` requires extra work to handle,
    // which we leave for now for simplicity.
    if thread.join().is_err() {
        return Err(SpinError::JoinSpinnerThreadFailed);
    }

    Ok(result)
}

// Characters adapted from
// <https://github.com/6/braille-pattern-cli-loading-indicator>.
const CYCLE_CHARS: &[char] = &[
    '⣷',
    '⣯',
    '⣟',
    '⡿',
    '⢿',
    '⣻',
    '⣽',
    '⣾',
];

const BACKSPACE: char = '\x08';

// NOTE These codes may not be portable.
const RESET_CURSOR: &str = "\x1b[1G";
const CLEAR_LINE: &str = "\x1b[2K";

#[derive(Debug, Snafu)]
pub enum SpinError {
    SendEndSignalFailed{source: SendError<()>},
    JoinSpinnerThreadFailed,
}
