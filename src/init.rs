// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::ffi::OsStr;
use std::fs;
use std::io::Error as IoError;
use std::mem;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;
use std::str;
use std::str::Utf8Error;

use snafu::OptionExt;
use snafu::ResultExt;
use snafu::Snafu;

use crate::run_in;
use crate::run_in::AssertRunError;

pub fn parse_templates_source(raw_source: &str)
    -> Result<TemplatesSource, ParseTemplatesSourceError>
{
    let first_colon = raw_source.find(':')
        .context(NoColonInSource)?;

    let (source_type, raw_source_url) = raw_source.split_at(first_colon);

    // TODO Consider whether to replace `unwrap()` with a "dev error".
    let source_location = raw_source_url
        .strip_prefix(':')
        .unwrap()
        .to_string();

    if source_type == "git" {
        Ok(TemplatesSource::Git(GitTemplatesSource::new(source_location)))
    } else if source_type == "dir" {
        Ok(TemplatesSource::Dir(DirTemplatesSource::new(source_location)))
    } else {
        let source_type = source_type.to_string();

        Err(ParseTemplatesSourceError::UnsupportedSourceType{source_type})
    }
}

#[derive(Debug, Snafu)]
pub enum ParseTemplatesSourceError {
    #[snafu(display("The templates source must contain ':'"))]
    NoColonInSource,
    #[snafu(display("Unsupported templates source type: {}", source_type))]
    UnsupportedSourceType{source_type: String},
}

pub enum TemplatesSource {
    Git(GitTemplatesSource),
    Dir(DirTemplatesSource),
}

impl TemplatesSource {
    fn clone_to(&self, dir: &Path) -> Result<(), CloneToError> {
        match self {
            Self::Git(s) =>
                s.clone_to(dir)
                    .context(GitCloneToFailed),
            Self::Dir(s) =>
                s.clone_to(dir)
                    .context(DirCloneToFailed),
        }
    }
}

#[derive(Debug, Snafu)]
pub enum CloneToError {
    #[snafu(display("{}", source))]
    GitCloneToFailed{source: GitCloneToError},
    #[snafu(display("{}", source))]
    DirCloneToFailed{source: DirCloneToError},
}

pub struct GitTemplatesSource {
    url: String,
}

impl GitTemplatesSource {
    fn new(url: String) -> Self {
        Self{url}
    }

    fn clone_to(&self, dir: &Path) -> Result<(), GitCloneToError> {
        assert_run_in_dir(dir, "git", &["clone", self.url.as_str(), "."])
            .context(GitCloneFailed{url: self.url.clone()})?;

        Ok(())
    }
}

#[derive(Debug, Snafu)]
pub enum GitCloneToError {
    #[snafu(display("Couldn't clone Git repository '{}': {}", url, source))]
    // TODO Consider whether to include `url` in this variant.
    GitCloneFailed{source: AssertRunError, url: String},
}

pub struct DirTemplatesSource {
    path: String,
}

impl DirTemplatesSource {
    fn new(path: String) -> Self {
        Self{path}
    }

    fn clone_to(&self, dir: &Path) -> Result<(), DirCloneToError> {
        // NOTE `remove_dir` doesn't remove `dir` if it isn't empty, which is
        // intended behaviour for this method.
        fs::remove_dir(dir)
            .context(RemoveDirFailed{path: self.path.clone()})?;

        // TODO Ideally the arguments to `assert_run` should use `&OsStr`s, so
        // `dir.as_os_str()` could be used without the potential for failure.
        let raw_dir = dir.to_str()
            .context(InvalidUtf8Dir{path: self.path.clone()})?;

        run_in::assert_run("cp", &["-r", self.path.as_str(), raw_dir])
            .context(CopyDirFailed{path: self.path.clone()})?;

        Ok(())
    }
}

#[derive(Debug, Snafu)]
pub enum DirCloneToError {
    // TODO Consider whether to include `path` in these variants.
    #[snafu(display(
        "Couldn't clone directory '{}': invalid UTF-8 in target path",
        path,
    ))]
    InvalidUtf8Dir{path: String},
    #[snafu(display("Couldn't clone directory '{}': {}", path, source))]
    RemoveDirFailed{source: IoError, path: String},
    #[snafu(display("Couldn't copy directory '{}': {}", path, source))]
    CopyDirFailed{source: AssertRunError, path: String},
}

pub fn init(
    logger: &mut dyn FileActionLogger,
    source: &TemplatesSource,
    template: &str,
    dock_file: &Path,
)
    -> Result<(), InitError>
{
    // TODO Use a `DOCK_CONFIG_YAML` to locate a `dock_config.yaml`, which can
    // define where templates can be cached. Not having this file can cause a
    // message to be displayed suggesting to create such a file.

    // TODO Check that a Dock file doesn't already exist.
    if dock_file.exists() {
        return Err(InitError::DockFileAlreadyExists);
    }

    // TODO Avoid creating a temporary directory on each run.
    let output = run_in::assert_run("mktemp", &["--directory"])
        .context(CreateTmpDirFailed)?;

    let raw_tmp_dir = str::from_utf8(&output.stdout)
        .context(TempDirAsUtf8Failed)?;
    let raw_tmp_dir = raw_tmp_dir.trim_end();
    let tmp_dir = PathBuf::from(raw_tmp_dir);

    source.clone_to(&tmp_dir)
        .context(CloneSourceFailed{dest: tmp_dir.clone()})?;

    let mut template_dir = tmp_dir;
    template_dir.push(template);

    let entries = fs::read_dir(&template_dir)
        .context(ReadTemplateDirFailed{template_dir})?;

    for maybe_entry in entries {
        let src = maybe_entry
            .context(ReadTemplateEntryFailed)?;

        let tgt_name = src.file_name();
        let tgt = Path::new(&tgt_name);

        if PathBuf::from(&tgt_name).exists() {
            // We ignore the error returned from logging the action.
            mem::drop(logger.log_file_action(tgt, FileAction::Skip));

            continue;
        }

        fs::copy(src.path(), tgt)
            .context(CopyTemplateFileFailed{path: src.path()})?;

        // We ignore the error returned from logging the action.
        mem::drop(logger.log_file_action(tgt, FileAction::Create));
    }

    Ok(())
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
pub enum InitError {
    #[snafu(display("The current directory already contains a Dock file"))]
    DockFileAlreadyExists,
    #[snafu(display("Couldn't create temporary directory: {}", source))]
    CreateTmpDirFailed{source: AssertRunError},
    #[snafu(display(
        "Couldn't convert temporary directory name to UTF-8: {}",
        source,
    ))]
    TempDirAsUtf8Failed{source: Utf8Error},
    #[snafu(display(
        "Couldn't clone templates source to '{}': {}",
        dest.display(),
        source,
    ))]
    CloneSourceFailed{source: CloneToError, dest: PathBuf},
    #[snafu(display(
        "Couldn't read template directory '{}': {}",
        template_dir.display(),
        source,
    ))]
    ReadTemplateDirFailed{source: IoError, template_dir: PathBuf},
    #[snafu(display("Couldn't read template entry: {}", source))]
    ReadTemplateEntryFailed{source: IoError},
    #[snafu(display(
        "Couldn't copy template file '{}': {}",
        path.display(),
        source,
    ))]
    CopyTemplateFileFailed{source: IoError, path: PathBuf},
}

// TODO Mostly duplicated from `crate::run_in`.
fn assert_run_in_dir<I, S>(dir: &Path, prog: &str, args: I)
    -> Result<Output, AssertRunError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let maybe_output =
        Command::new(prog)
            .args(args)
            .current_dir(dir)
            .output();

    let output =
        match maybe_output {
            Ok(output) => output,
            Err(source) => return Err(AssertRunError::RunFailed{source}),
        };

    if !output.status.success() {
        return Err(AssertRunError::NonZeroExit{output});
    }

    Ok(output)
}

pub trait FileActionLogger {
    fn log_file_action(&mut self, file: &Path, action: FileAction)
        -> Result<(), IoError>;
}

pub enum FileAction {
    Create,
    Skip,
}
