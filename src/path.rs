// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

///! Types for managing absolute and relative paths.
///!
///! Provides alternatives to the standard `Path` and `PathBuf` primitives that
///! allow distinguishing between absolute and relative paths. Note that these
///! types store paths in canonical form, and don't support having path
///! traversal components (such as `.` and `..`) in paths.

use std::char;
use std::ffi::OsString;
use std::fmt::Debug;
use std::iter::FromIterator;
use std::path;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::str;

use snafu::OptionExt;
use snafu::Snafu;

pub type AbsPath = Vec<OsString>;

/// Returns the `AbsPath` parsed from `p`. `p` must begin with a "root
/// directory" component.
pub fn parse_abs_path(p: &str) -> Result<AbsPath, NewAbsPathError> {
    abs_path_from_path_buf(Path::new(p))
}

#[derive(Debug, Snafu)]
pub enum NewAbsPathError {
    #[snafu(display("The absolute path was empty"))]
    EmptyAbsPath,
    // TODO We would ideally add the path component as a field on
    // `NoRootDirPrefix` and `SpecialComponentInAbsPath` to track the component
    // that was unexpected. However, the current version of `Snafu` being used
    // ["cannot use lifetime-parameterized errors as
    // sources"](https://github.com/shepmaster/snafu/issues/99), so we omit
    // this field for now.
    #[snafu(display("The absolute path didn't start with `/`"))]
    NoRootDirPrefix,
    #[snafu(display(
        "The absolute path contained a special component, such as `.` or `..`"
    ))]
    SpecialComponentInAbsPath,
}

pub fn abs_path_from_path_buf(p: &Path) -> Result<AbsPath, NewAbsPathError> {
    let mut components = p.components();

    let component = components.next()
        .context(EmptyAbsPath)?;

    if component != Component::RootDir {
        return Err(NewAbsPathError::NoRootDirPrefix);
    }

    let mut abs_path = vec![];
    for component in components {
        if let Component::Normal(c) = component {
            abs_path.push(c.to_os_string());
        } else {
            return Err(NewAbsPathError::SpecialComponentInAbsPath);
        }
    }

    Ok(abs_path)
}

// TODO `abs_path_display` should ideally return an error instead of `None` if
// there is a problem rendering a component of the path.
pub fn abs_path_display(abs_path: AbsPathRef) -> Option<String> {
    if abs_path.is_empty() {
        return Some(path::MAIN_SEPARATOR.to_string());
    }

    let mut string = String::new();
    for component in abs_path {
        string += &path::MAIN_SEPARATOR.to_string();
        string += component.to_str()?;
    }

    Some(string)
}

pub fn abs_path_display_lossy(abs_path: AbsPathRef) -> String {
    if abs_path.is_empty() {
        return path::MAIN_SEPARATOR.to_string();
    }

    let mut string = String::new();
    for component in abs_path {
        string += &path::MAIN_SEPARATOR.to_string();
        if let Some(s) = component.to_str() {
            string += s;
        } else {
            string += &char::REPLACEMENT_CHARACTER.to_string();
        }
    }

    string
}

pub fn abs_path_extend(abs_path: &mut AbsPath, rel_path: RelPath) {
    abs_path.extend(rel_path);
}

pub fn abs_path_to_path_buf(abs_path: AbsPath) -> PathBuf {
    let mut p = PathBuf::new();
    p.push(Component::RootDir);
    p.push(PathBuf::from_iter(abs_path));

    p
}

pub type AbsPathRef<'a> = &'a [OsString];

pub type RelPath = Vec<OsString>;

/// Returns the `RelPath` derived from `p`. `p` must begin with a "current
/// directory" component (i.e. `.`).
pub fn rel_path_from_path_buf(p: &Path) -> Result<RelPath, NewRelPathError> {
    let mut components = p.components();

    let component = components.next()
        .context(EmptyRelPath)?;

    if component != Component::CurDir {
        return Err(NewRelPathError::NoCurDirPrefix);
    }

    let mut rel_path = vec![];
    for component in components {
        if let Component::Normal(c) = component {
            rel_path.push(c.to_os_string());
        } else {
            return Err(NewRelPathError::SpecialComponentInRelPath);
        }
    }

    Ok(rel_path)
}

#[derive(Debug, Snafu)]
pub enum NewRelPathError {
    #[snafu(display("The relative path was empty"))]
    EmptyRelPath,
    // TODO See `NewAbsPathError` for more details on adding `Component` fields
    // in error variants.
    #[snafu(display("The relative path didn't start with `.`"))]
    NoCurDirPrefix,
    #[snafu(display(
        "The relative path contained a special component, such as `.` or `..`"
    ))]
    SpecialComponentInRelPath,
}
