// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::error::Error as StdError;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::fmt::Display;
use std::fs;
use std::fs::DirEntry;
use std::fs::FileType;
use std::io::Error as IoError;
use std::io::ErrorKind;
use std::mem;
use std::path::Path;
use std::path::PathBuf;
use std::path::StripPrefixError;
use std::process::Command;
use std::process::Output;
use std::str;
use std::str::Utf8Error;
use std::string::ToString;

use snafu::OptionExt;
use snafu::ResultExt;
use snafu::Snafu;

use crate::canon_path::NewRelPathError;
use crate::canon_path::RelPath;
use crate::run_in;
use crate::run_in::AssertRunError;

pub fn parse_templates_source(raw_source: &str)
    -> Result<TemplatesSource, ParseTemplatesSourceError>
{
    let parts = split_templates_source(raw_source)
        .context(SplitTemplatesSourceFailed)?;

    let scheme =
        if parts.scheme == "git" {
            let source = GitTemplatesSource::new(parts.addr, parts.reference);

            TemplatesSourceScheme::Git(source)
        } else if parts.scheme == "dir" {
            if parts.reference != "-" {
                return Err(ParseTemplatesSourceError::UnsupportedDirReference{
                    reference: parts.reference,
                });
            }

            TemplatesSourceScheme::Dir(DirTemplatesSource::new(parts.addr))
        } else {
            return Err(ParseTemplatesSourceError::UnsupportedSourceType{
                source_scheme: parts.scheme,
            });
        };

    let subdir_path = PathBuf::from(parts.subdir);
    let subdir = RelPath::try_from(subdir_path.clone())
        .context(SubdirToRelPathFailed{subdir_path})?;

    Ok(TemplatesSource{scheme, subdir})
}

#[derive(Debug, Snafu)]
pub enum ParseTemplatesSourceError {
    #[snafu(display("Couldn't split the templates source: {}", source))]
    SplitTemplatesSourceFailed{source: SplitTemplatesSourceError},
    #[snafu(display(
        "'{}' can't be used as a reference for the 'dir' scheme (only '-' is \
         supported)",
        reference,
    ))]
    UnsupportedDirReference{reference: String},
    #[snafu(display("Unsupported templates source scheme: {}", source_scheme))]
    UnsupportedSourceType{source_scheme: String},
    #[snafu(display(
        "Couldn't convert '{}' to a relative path: {}",
        subdir_path.display(),
        source,
    ))]
    SubdirToRelPathFailed{source: NewRelPathError, subdir_path: PathBuf}
}

fn split_templates_source(raw_source: &str)
    -> Result<TemplatesSourceParts, SplitTemplatesSourceError>
{
    let parts: Vec<&str> = raw_source.split(':').collect();

    let n = parts.len();
    if n < 4 {
        // TODO Add `n` to this error.
        return Err(SplitTemplatesSourceError::TooFewColons);
    }

    Ok(TemplatesSourceParts{
        scheme: parts[0].to_string(),
        addr: parts[1..n-2].join(":"),
        reference: parts[n-2].to_string(),
        subdir: parts[n-1].to_string(),
    })
}

#[derive(Debug, Snafu)]
pub enum SplitTemplatesSourceError {
    #[snafu(display("The templates source contained too few ':'"))]
    TooFewColons,
}

struct TemplatesSourceParts {
    scheme: String,
    addr: String,
    reference: String,
    subdir: String,
}

pub struct TemplatesSource {
    scheme: TemplatesSourceScheme,
    subdir: RelPath,
}

pub enum TemplatesSourceScheme {
    Git(GitTemplatesSource),
    Dir(DirTemplatesSource),
}

impl TemplatesSourceScheme {
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
    reference: String,
}

impl GitTemplatesSource {
    fn new(url: String, reference: String) -> Self {
        Self{url, reference}
    }

    fn clone_to(&self, dir: &Path) -> Result<(), GitCloneToError> {
        // This optimised flow for cloning a single reference is taken from
        // <https://stackoverflow.com/a/71911631>.

        let args = vec![
            "clone",
            "--branch",
            &self.reference,
            "--no-tags",
            "--depth=1",
            &self.url,
            ".",
        ];

        assert_run_in_dir(dir, "git", &args)
            .with_context(|| {
                let args: Vec<String> =
                    args.iter()
                        .map(ToString::to_string)
                        .collect();

                GitCommandFailed{args}
            })?;

        Ok(())
    }
}

#[derive(Debug, Snafu)]
pub enum GitCloneToError {
    #[snafu(display(
        "Git command failed ('git {}'): {}",
        args.join(" "),
        source,
    ))]
    // TODO Consider whether to include `url` in this variant.
    GitCommandFailed{source: AssertRunError, args: Vec<String>},
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
    target_dir: &Path,
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

    source.scheme.clone_to(&tmp_dir)
        .context(CloneSourceFailed{dest: tmp_dir.clone()})?;

    let subdir: PathBuf = source.subdir.iter().collect();

    let mut template_dir = tmp_dir;
    template_dir.push(subdir);
    template_dir.push(template);

    fs_deep_copy(logger, &template_dir, target_dir)
        .context(CopyTemplateFailed{start_dir: template_dir})?;

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
        "Couldn't copy template from '{}': {}",
        start_dir.display(),
        source,
    ))]
    CopyTemplateFailed{source: WalkError<FsDeepCopyError>, start_dir: PathBuf},
}

fn fs_deep_copy(logger: &mut dyn FileActionLogger, src: &Path, tgt: &Path)
    -> Result<(), WalkError<FsDeepCopyError>>
{
    walk(
        src,
        |entry, file_type| {
            let entry_path = entry.path();
            let rel_path = entry_path.strip_prefix(src)
                .context(DevErrStripPrefixFailed{
                    entry_path: entry_path.clone(),
                    prefix: src,
                })?;
            let tgt = tgt.join(rel_path);

            let mut action = FileAction::Create;
            if file_type.is_dir() {
                let result = fs::create_dir(&tgt);

                if let Err(source) = result {
                    if source.kind() != ErrorKind::AlreadyExists {
                        return Err(FsDeepCopyError::CreateDirFailed{
                            source,
                            path: tgt.clone(),
                        });
                    }

                    action = FileAction::Skip;
                }
            } else if tgt.exists() {
                action = FileAction::Skip;
            } else {
                fs::copy(&entry_path, &tgt)
                    .context(CopyFileFailed{path: entry_path})?;
            }

            // We ignore the error returned from logging the action.
            mem::drop(logger.log_file_action(&tgt, action));

            Ok(())
        },
    )
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
pub enum FsDeepCopyError {
    #[snafu(display(
        "Couldn't create target directory '{}': {}",
        path.display(),
        source,
    ))]
    CreateDirFailed{source: IoError, path: PathBuf},
    #[snafu(display(
        "Couldn't copy template file '{}': {}",
        path.display(),
        source,
    ))]
    CopyFileFailed{source: IoError, path: PathBuf},

    #[snafu(display(
        "Dev Error: Couldn't strip prefix '{}' from '{}': {}",
        prefix.display(),
        entry_path.display(),
        source,
    ))]
    DevErrStripPrefixFailed{
        source: StripPrefixError,
        entry_path: PathBuf,
        prefix: PathBuf,
    },
}

fn walk<F, E>(dir: &Path, mut f: F) -> Result<(), WalkError<E>>
where
    F: FnMut(&DirEntry, FileType) -> Result<(), E>,
    E: 'static + Debug + Display + StdError,
{
    let entries = fs::read_dir(&dir)
        .context(ReadDirFailed{dir})?;

    let mut frontier: Vec<Result<DirEntry, IoError>> = entries.collect();

    while let Some(maybe_entry) = frontier.pop() {

        // TODO Keep the directory that the entry came from, so that it can be
        // added to the error, or resolve entries as they're added to the
        // frontier.
        let entry = maybe_entry
            .context(ReadEntryFailed)?;

        let file_type = entry.file_type()
            // NOTE We can't add `entry` to the error for now because
            // `DirEntry` doesn't implement `clone()`, and the `entry` is
            // needed as a parameter to `f`.
            .context(GetEntryFileTypeFailed{entry_path: entry.path()})?;

        f(&entry, file_type)
            // NOTE Same as above.
            .context(CallFailed{entry_path: entry.path()})?;

        if file_type.is_dir() {
            let entry_path = entry.path();

            let entries = fs::read_dir(&entry_path)
                .context(ReadDirFailed{dir: entry_path})?;

            frontier.extend(entries);
        }
    }

    Ok(())
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
pub enum WalkError<E: 'static + Debug + Display + StdError> {
    #[snafu(display(
        "Couldn't read directory '{}': {}",
        dir.display(),
        source,
    ))]
    ReadDirFailed{source: IoError, dir: PathBuf},
    #[snafu(display("Couldn't read directory entry: {}", source))]
    ReadEntryFailed{source: IoError},
    #[snafu(display(
        "Couldn't get file type of directory entry '{}': {}",
        entry_path.display(),
        source,
    ))]
    GetEntryFileTypeFailed{source: IoError, entry_path: PathBuf},
    #[snafu(display("{}", source))]
    CallFailed{source: E, entry_path: PathBuf},
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
