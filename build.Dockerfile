# Copyright 2021-2023 Sean Kelleher. All rights reserved.
# Use of this source code is governed by an MIT
# licence that can be found in the LICENCE file.

FROM rust:1.60.0-buster

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
        | VERSION=19.03.8 \
            sh

RUN \
    cargo install \
        --version=1.1.3 \
        --locked \
        just

ENV DOCK_DEFAULT_TEMPLATES_SOURCE git:https://github.com/ezanmoto/dock_init_templates.git:0.1:./templates
