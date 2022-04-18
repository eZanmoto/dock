# Copyright 2021-2022 Sean Kelleher. All rights reserved.
# Use of this source code is governed by an MIT
# licence that can be found in the LICENCE file.

# `$0` runs a command in the build environment.

set -o errexit

org='ezanmoto'
proj='dock'
build_img="$org/$proj.build"

bash scripts/docker_rbuild.sh \
        "$build_img" \
        "latest" \
        --file='build.Dockerfile' \
        scripts

vol_name="$org.$proj.cargo_cache"
vol_dir='/cargo'

docker run \
        --rm \
        --mount="type=volume,src=$vol_name,dst=$vol_dir" \
        "$build_img:latest" \
        chmod 0777 "$vol_dir"

docker_sock='/var/run/docker.sock'

# `stat --format='%s'` outputs the group ID for the given file.
host_docker_group_id=$(
    stat \
        --format='%g' \
        "$docker_sock"
)

workdir_host_path="$(pwd)"
workdir_mount_path="/app"

# `DOCK_HOSTPATHS` is defined to be in the format that `dock` expects in order
# to support nested bind mounts.
docker run \
        --interactive \
        --tty \
        --rm \
        --mount="type=bind,src=$docker_sock,dst=$docker_sock" \
        --group-add="$host_docker_group_id" \
        --mount="type=volume,src=$vol_name,dst=$vol_dir" \
        --env="CARGO_HOME=$vol_dir" \
        --user="$(id --user):$(id --group)" \
        --mount="type=bind,src=$workdir_host_path,dst=$workdir_mount_path" \
        --workdir="$workdir_mount_path" \
        --env="DOCK_HOSTPATHS=$workdir_host_path:$workdir_mount_path" \
        "$build_img:latest" \
        "$@"
