// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use crate::line_matcher::AssertError;
use crate::line_matcher::LineMatcher;

#[derive(Debug)]
pub struct DockerBuild {
    tagged_name: String,
    layers: Vec<DockerBuildLayer>,
}

impl DockerBuild {
    // `parse_from_stdout` returns `Ok(None)` if the build parsed from `stdout`
    // was unsuccessful.
    pub fn parse_from_stdout<'a>(lines: &mut LineMatcher)
        -> Result<Option<DockerBuild>, AssertError<'a>>
    {
        lines.assert_prefix("Sending build context to Docker daemon ")?;
        lines.assert_prefix("Step 1/")?;
        lines.assert_prefix(" ---> ")?;

        let mut layers = vec![];
        while !lines.skip_if_starts_with("Successfully built ") {

            lines.assert_prefix("Step ")?;

            if !lines.skip_if_starts_with(" ---> Using cache")
                && lines.skip_if_starts_with(" ---> Running in ") {

                let msg = "Removing intermediate container ";
                lines.assert_skip_until_starts_with(msg)?;

                // If a command run in a layer returns non-zero then the STDOUT
                // will finish after outputting the above message, because
                // `docker build` is internally run with `--force-rm`.
                if lines.peek().is_none() {
                    return Ok(None);
                }
            }

            let layer_id = lines.assert_strip_prefix(" ---> ")?;
            layers.push(DockerBuildLayer{id: layer_id.to_string()});
        }

        let tagged_name = lines.assert_strip_prefix("Successfully tagged ")?
            .to_string();

        lines.assert_eof()?;

        Ok(Some(DockerBuild{layers, tagged_name}))
    }

    pub fn tagged_name(&self) -> &str {
        &self.tagged_name
    }

    // TODO Consider returning `&str`.
    pub fn img_id(&self) -> String {
        self.layers
            .last()
            .expect("Docker build had no layers")
            .id
            .clone()
    }
}

#[derive(Debug)]
pub struct DockerBuildLayer {
    id: String,
}
