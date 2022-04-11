#!/bin/sh

# Copyright 2022 Sean Kelleher. All rights reserved.
# Use of this source code is governed by an MIT
# licence that can be found in the LICENCE file.

# `$0 <prefix>` removes all Docker images whose name starts with `prefix`.

set -o errexit

if [ $# -ne 1 ] ; then
    echo "usage: $0 <prefix>" >&2
    exit 1
fi

prefix="$1"

images=$(
    docker images \
        | grep \
            "$prefix" \
        | cut \
            --delimiter=' ' \
            --field=1
)

if [ ! -z "$images" ] ; then
    docker rmi $images
fi
