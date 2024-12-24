# Copyright 2022-2024 Sean Kelleher. All rights reserved.
# Use of this source code is governed by an MIT
# licence that can be found in the LICENCE file.

FROM ezanmoto/dock.build:latest

RUN \
    cargo install \
        --version=0.2.5 cross

ENV CROSS_DOCKER_IN_DOCKER=true
