# Copyright 2021-2022 Sean Kelleher. All rights reserved.
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
    mkdir \
            '/tmp/just' \
        && curl \
            --fail \
            --show-error \
            --silent \
            --location \
            --proto '=https' \
            --tlsv1.2 \
            'https://github.com/casey/just/releases/download/1.1.3/just-1.1.3-x86_64-unknown-linux-musl.tar.gz' \
        | tar \
            --extract \
            --gzip \
            --directory='/tmp/just' \
        && install \
            --mode 755 \
            '/tmp/just/just' \
            '/usr/local/bin'
