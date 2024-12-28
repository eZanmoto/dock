// Copyright 2024 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::ffi::OsStr;
use std::fmt::Debug;
use std::process::Stdio;
use std::str;

use snafu::ResultExt;
use snafu::Snafu;

use crate::logging_process;
use crate::logging_process::CommandLogger;
use crate::logging_process::RunError;
use crate::run_in;
use crate::run_in::FindAndParseDockConfigError;

pub fn clean(
    logger: &mut dyn CommandLogger,
    dock_file_name: &str,
    remove_images: bool,
    remove_volumes: bool,
)
    -> Result<(), CleanError>
{
    let (_, conf) = run_in::find_and_parse_dock_config(dock_file_name)
        .context(FindAndParseDockConfigFailed{dock_file_name})?;

    for (env_name, env) in conf.environments {
        if remove_volumes {
            let cache_vol_name_prefix = run_in::cache_vol_name_prefix(
                &conf.organisation,
                &conf.project,
                &env_name,
            );

            if let Some(vols) = env.cache_volumes {
                for (vol_name, _) in vols {
                    let name = run_in::cache_vol_name(
                        &cache_vol_name_prefix,
                        &vol_name,
                    );

                    let prog = OsStr::new("docker");
                    let raw_rm_args = &["volume", "rm", name.as_str()];
                    let rm_args = run_in::new_os_strs(raw_rm_args);
                    let _ = logging_process::run(
                        logger,
                        prog,
                        &rm_args,
                        Stdio::null(),
                    )
                        .context(RemoveVolumeFailed{name})?;
                }
            }
        }

        if remove_images {
            let img_name = run_in::image_name(
                &conf.organisation,
                &conf.project,
                &env_name,
            );
            let name = img_name + ":latest";
            // TODO Handle cache image.

            let prog = OsStr::new("docker");
            let raw_rmi_args = &["rmi", name.as_str()];
            let rmi_args = run_in::new_os_strs(raw_rmi_args);
            let _ = logging_process::run(
                logger,
                prog,
                &rmi_args,
                Stdio::null(),
            )
                .context(RemoveImageFailed{name})?;
        }
    }

    Ok(())
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Snafu)]
pub enum CleanError {
    #[snafu(display(
        "Couldn't find and parse '{}': {}",
        dock_file_name,
        source,
    ))]
    FindAndParseDockConfigFailed{
        source: FindAndParseDockConfigError,
        dock_file_name: String,
    },
    #[snafu(display("Couldn't remove volume '{}': {}", name, source))]
    RemoveVolumeFailed{
        source: RunError,
        name: String,
    },
    #[snafu(display("Couldn't remove image '{}': {}", name, source))]
    RemoveImageFailed{
        source: RunError,
        name: String,
    },
}
