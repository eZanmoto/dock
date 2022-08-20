#!/bin/sh

# Copyright 2022 Sean Kelleher. All rights reserved.
# Use of this source code is governed by an MIT
# licence that can be found in the LICENCE file.

# `$0 <prefix>` removes all Docker images whose name starts with `prefix`,
# and removes any containers created from them.

set -o errexit

if [ $# -ne 1 ] ; then
    echo "usage: $0 <prefix>" >&2
    exit 1
fi

prefix="$1"

tagged_images="$(
    docker images \
        | grep "$prefix" \
        | sed 's/ \+/ /g' \
        | cut \
            --delimiter=' ' \
            --field=1-2 \
        | sed 's/ /:/'
)"

if [ ! -z "$tagged_images" ] ; then
    for img in $tagged_images ; do
        cont_ids="$(
            docker ps \
                --all \
                --filter=ancestor="$img" \
                --quiet
        )"

        if [ ! -z "$cont_ids" ] ; then
            docker rm \
                --force \
                $cont_ids
        fi
    done

    docker rmi $tagged_images
fi
