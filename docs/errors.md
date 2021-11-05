Error Handling
==============

About
-----

This document contains information on the error handling approach taken in this
codebase.

Introduction
------------

This project identifies two types of error value. An error variant that contains
a nested error is considered a "failed operation" error. For example:

    enum InstallError<E> {
        ...
        ReadStateFileFailed{source: IoError, path: PathBuf},
        ...
    }

Any other error variant is considered a "root" error. For example:

    enum InstallError<E> {
        ...
        NoDepsFileFound,
        ...
    }

    enum ParseDepsError {
        InvalidDependencySpec{line_num: usize, line: String},
        UnknownTool{line_num: usize, dep_name: String, tool_name: String},
    }

SNAFU
-----

This project uses [SNAFU](https://crates.io/crates/snafu) for handling errors.
Any type that will be used to signal an error (particularly if used as the
second type in a `Result`) must `derive` `Snafu`. This is required for error
types that contain failed operation errors, so that `.context()` methods can be
used to succinctly return errors. This is also required for error types that
contain root errors, even though such errors will rarely be constructed using
the `.context()` shortcut. The reason for deriving SNAFU in this case is to
allow such errors to be easily nested; SNAFU can only use `Error`s as `source`
values, and deriving SNAFU is a quick way of automatically implementing this
requirement on error types that only define root errors.
