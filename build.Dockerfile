# Copyright 2021-2024 Sean Kelleher. All rights reserved.
# Use of this source code is governed by an MIT
# licence that can be found in the LICENCE file.

FROM rust:1.83.0-bullseye

RUN \
    rustup component add \
            clippy

RUN \
    curl \
            --fail \
            --show-error \
            --silent \
            --location \
            https://get.docker.com \
        | VERSION=23.0.3 \
            sh

# We create the `/.docker` directory manually because it's used by the latest
# version of `docker`, and we make it writeable by everyone so that mounted
# users can access `docker`.
RUN \
    mkdir /.docker \
        && chmod 0777 /.docker

RUN \
    cargo install \
        --version=1.1.3 \
        --locked \
        just

ENV DOCK_DEFAULT_TEMPLATES_SOURCE git:https://github.com/ezanmoto/dock_init_templates.git:0.1:./templates
