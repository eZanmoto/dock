#!/bin/sh

# Copyright 2022 Sean Kelleher. All rights reserved.
# Use of this source code is governed by an MIT
# licence that can be found in the LICENCE file.

# `$0 <prefix>` removes all Docker volumes whose name starts with `prefix`.

set -o errexit

if [ $# -ne 1 ] ; then
    echo "usage: $0 <prefix>" >&2
    exit 1
fi

prefix="$1"

vols=$(
    docker volume ls \
        | grep \
            " $prefix" \
        | sed \
            -e 's/^local *//'
)

if [ ! -z "$vols" ] ; then
    docker volume rm $vols
fi
