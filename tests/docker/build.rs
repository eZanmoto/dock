// Copyright 2022-2024 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use regex::Regex;

use crate::line_matcher::AssertError;
use crate::line_matcher::LineMatcher;

#[derive(Debug)]
pub struct DockerBuild {
    tagged_name: String,
    img_id: String,
}

impl DockerBuild {
    // `parse_from_stderr` returns `Ok(None)` if the build parsed from `stderr`
    // was unsuccessful.
    //
    // NOTE This parse depends on the specific STDERR returned by the Docker
    // client. This parse able to handle `Docker Engine - Community` version
    // `23.0.3` of the Docker client.
    pub fn parse_from_stderr(lines: &mut LineMatcher)
        -> Result<Option<DockerBuild>, AssertError>
    {
        let img_id;
        loop {
            let line =
                if let Some(line) = lines.peek() {
                    line
                } else {
                    return Err(AssertError::UnexpectedEof);
                };

            // The following marker indicates that a command run in a layer
            // returned non-zero and so the overall build failed.
            let re = Regex::new(r"^------$")
                .expect("couldn't construct error marker matcher");

            if re.is_match(line) {
                return Ok(None);
            }

            let re = Regex::new(r"#[0-9]+ writing image sha256:([a-z0-9]+)")
                .expect("couldn't construct image ID matcher");

            if let Some(cap) = re.captures(line) {
                img_id =
                    cap
                        .get(1)
                        .expect("couldn't get capture group")
                        .as_str()
                        .to_string();

                break;
            }

            lines.next_line();
        }

        let tagged_name;
        loop {
            let line =
                if let Some(line) = lines.peek() {
                    line
                } else {
                    return Err(AssertError::UnexpectedEof);
                };

            let re = Regex::new(r"#[0-9]+ naming to docker.io/([^ ]+)")
                .expect("couldn't construct image name matcher");

            if let Some(cap) = re.captures(line) {
                tagged_name =
                    cap
                        .get(1)
                        .expect("couldn't get capture group")
                        .as_str()
                        .to_string();

                break;
            }

            lines.next_line();
        }

        Ok(Some(DockerBuild{tagged_name, img_id}))
    }

    pub fn tagged_name(&self) -> &str {
        &self.tagged_name
    }

    pub fn img_id(&self) -> &str {
        &self.img_id
    }
}
