# Copyright 2022 Sean Kelleher. All rights reserved.
# Use of this source code is governed by an MIT
# licence that can be found in the LICENCE file.

FROM alpine:3.14.2

# We run an `echo` command in order to give this image a unique ID. See
# `tests/cli/README.md` for more information.
RUN \
    echo 'dock_base_test_image'
