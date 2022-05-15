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
use std::ops::Deref;
use std::path;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::str;

use snafu::OptionExt;
use snafu::Snafu;

#[derive(Clone, Debug)]
pub struct AbsPath {
    components: Vec<OsString>,
}

impl AbsPath {
    /// Returns the `AbsPath` parsed from `p`. `p` must begin with a "root
    /// directory" component.
    pub fn parse(s: &str) -> Result<Self, NewAbsPathError> {
        AbsPath::try_from(Path::new(s).to_path_buf())
    }

    // TODO `abs_path_display` should ideally return an error instead of `None`
    // if there is a problem rendering a component of the path.
    pub fn display(&self) -> Option<String> {
        if self.components.is_empty() {
            return Some(path::MAIN_SEPARATOR.to_string());
        }

        let mut string = String::new();
        for component in &self.components {
            string += &path::MAIN_SEPARATOR.to_string();
            string += component.to_str()?;
        }

        Some(string)
    }

    pub fn display_lossy(&self) -> String {
        if self.components.is_empty() {
            return path::MAIN_SEPARATOR.to_string();
        }

        let mut string = String::new();
        for component in &self.components {
            string += &path::MAIN_SEPARATOR.to_string();
            if let Some(s) = component.to_str() {
                string += s;
            } else {
                string += &char::REPLACEMENT_CHARACTER.to_string();
            }
        }

        string
    }

    pub fn extend(&mut self, rel_path: RelPath) {
        // TODO Avoid accessing the `components` field of `rel_path` directly.
        self.components.extend(rel_path.components);
    }

    pub fn concat(&self, rel_path: &RelPath) -> Self {
        let mut p = self.clone();
        p.extend(rel_path.clone());

        p
    }
}

impl TryFrom<PathBuf> for AbsPath {
    type Error = NewAbsPathError;

    fn try_from(p: PathBuf) -> Result<Self, Self::Error> {
        let mut path_components = p.components();

        let component = path_components.next()
            .context(EmptyAbsPath)?;

        if component != Component::RootDir {
            return Err(NewAbsPathError::NoRootDirPrefix);
        }

        let mut components = vec![];
        for component in path_components {
            if let Component::Normal(c) = component {
                components.push(c.to_os_string());
            } else {
                return Err(NewAbsPathError::SpecialComponentInAbsPath);
            }
        }

        Ok(Self{components})
    }
}

impl From<Vec<OsString>> for AbsPath {
    fn from(components: Vec<OsString>) -> Self {
        Self{components}
    }
}

impl From<AbsPath> for PathBuf {
    fn from(ap: AbsPath) -> Self {
        let mut p = PathBuf::new();
        p.push(Component::RootDir);
        p.push(PathBuf::from_iter(ap.components));

        p
    }
}

impl Deref for AbsPath {
    type Target = Vec<OsString>;

    fn deref(&self) -> &Self::Target {
        &self.components
    }
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

#[derive(Clone, Debug)]
pub struct RelPath {
    components: Vec<OsString>,
}

impl From<Vec<OsString>> for RelPath {
    fn from(components: Vec<OsString>) -> Self {
        Self{components}
    }
}

impl TryFrom<PathBuf> for RelPath {
    type Error = NewRelPathError;

    /// Returns the `RelPath` derived from `p`. `p` must begin with a "current
    /// directory" component (i.e. `.`).
    fn try_from(p: PathBuf) -> Result<Self, Self::Error> {
        let mut path_components = p.components();

        let component = path_components.next()
            .context(EmptyRelPath)?;

        if component != Component::CurDir {
            return Err(NewRelPathError::NoCurDirPrefix);
        }

        let mut components = vec![];
        for component in path_components {
            if let Component::Normal(c) = component {
                components.push(c.to_os_string());
            } else {
                return Err(NewRelPathError::SpecialComponentInRelPath);
            }
        }

        Ok(Self{components})
    }
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

impl Deref for RelPath {
    type Target = Vec<OsString>;

    fn deref(&self) -> &Self::Target {
        &self.components
    }
}
