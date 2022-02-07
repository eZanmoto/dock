# Copyright 2022 Sean Kelleher. All rights reserved.
# Use of this source code is governed by an MIT
# licence that can be found in the LICENCE file.

# `$0 <image>` returns 1 if any Docker containers exists that are derived from
# `image`. `image` can be of the format `<image-name>[:<tag>]`, `<image-id>`, or
# `<image@digest>`.

if [ $# -ne 1 ] ; then
    echo "usage: $0 <image>" >&2
    exit 1
fi

image="$1"

cont_ids=$(
	docker ps \
		--all \
		--filter=ancestor="$image" \
        --quiet
)

if [ ! -z "$cont_ids" ] ; then
    echo -e "Containers descended from '$image' were found:\n"
    echo "$cont_ids" \
        | sed 's/^/    /'
    echo ''
    exit 1
fi
