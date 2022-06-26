# Copyright 2022 Sean Kelleher. All rights reserved.
# Use of this source code is governed by an MIT
# licence that can be found in the LICENCE file.

# `$0 <target> [--debug|-D]` builds a release binary of `dock` for the given
# `target`. Supported targets are those supported by
# [cross](https://github.com/rust-embedded/cross#supported-targets).

set -o errexit

if [ $# -lt 1 ] ; then
    echo "usage: $0 <target> [--debug|-D]" >&2
    exit 1
fi

target="$1"
shift

dock_flags=''
while [ $# -gt 0 ] ; do
    case "$1" in
        --debug|-D)
            dock_flags='--debug'
            shift
            ;;
        *)
            echo "usage: $0 <target> [--debug|-D]" >&2
            exit 1
            ;;
    esac
done

# NOTE `with_build_env.sh` performs the double duty of also building the
# `ezanmoto/dock.build` image, which the `ezanmoto/dock.cross` image depends on.
# TODO This implicit dependency should ideally be made explicit by allowing
# image dependencies to be defined in `dock.yaml`
#
# TODO Omitting `--release` with either `cross build` or `cross test` results in
# a build failure due to an appropriate version of GLIBC not being found.
bash scripts/with_build_env.sh \
    bash \
        -x \
        -c "
            cargo \
                    build \
                && target/debug/dock run-in cross \
                    $dock_flags \
                    cross build \
                        --release \
                        --target '$target'
        "
