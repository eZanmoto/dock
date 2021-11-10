# Copyright 2021 Sean Kelleher. All rights reserved.
# Use of this source code is governed by an MIT
# licence that can be found in the LICENCE file.

FROM rust:1.45.2-stretch

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
