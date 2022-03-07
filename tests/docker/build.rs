// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::str::Lines;

#[derive(Debug)]
pub struct DockerBuild {
    layers: Vec<DockerBuildLayer>
}

impl DockerBuild {
    // `assert_parse_from_stdout` returns `None` if the build parsed from
    // `stdout` was unsuccessful.
    pub fn assert_parse_from_stdout(stdout: &mut Lines, tagged_name: &str)
        -> Option<DockerBuild>
    {
        let mut line = stdout.next().expect("expected build context message");
        let exp_line = "Sending build context to Docker daemon";
        assert!(line.starts_with(exp_line), "unexpected prefix: {}", line);

        let mut last_layer_id: Option<String> = None;
        let mut layers = vec![];
        loop {
            line = stdout.next().expect("expected next layer or end message");
            if let Some(id) = last_layer_id {
                let exp_msg = format!("Successfully built {}", &id);
                if line.starts_with(&exp_msg) {
                    break;
                }
            }

            assert!(line.starts_with("Step "), "unexpected prefix: {}", line);
            line = stdout.next().expect("expected cache or run message");

            if line.starts_with(" ---> Using cache") {
                line = stdout.next().expect("expected run message");
            }

            if line.starts_with(" ---> Running in ") {
                let exp_msg = "expected end message or layer ID";
                line = stdout.next().expect(exp_msg);

                // If a command run in a layer returns non-zero then the STDOUT
                // will finish with the following message, because `docker
                // build` is internally run with `--force-rm`.
                if line.starts_with("Removing intermediate container ") {
                    assert_eq!(stdout.next(), None, "expected EOF");
                    return None
                }

                while !line.starts_with(" ---> ") {
                    line = stdout.next().expect("expected layer ID");
                }
            }

            let layer_id = line.strip_prefix(" ---> ").unwrap().to_string();
            last_layer_id = Some(layer_id.clone());
            layers.push(DockerBuildLayer{id: layer_id});
        }
        line = stdout.next().expect("expected trailing newline");

        assert_eq!(line, "Successfully tagged ".to_owned() + tagged_name);

        assert_eq!(stdout.next(), None);

        Some(DockerBuild{layers})
    }

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
