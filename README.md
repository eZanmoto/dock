Dock
====

About
-----

Dock defines a number of sub-commands to help use Docker containers as
environments.

An example usage would be when downloading a Rust project that has defined a
`dock.yaml` and `build.Dockerfile`. Building such a project should be as easy as
running the following command:

    dock run-in build cargo build

Dock takes care of re-creating the build environment and all the required system
dependencies using the `build.Dockerfile`, and runs `cargo build` in that
environment. Using directory and user "mounting" (configured using `dock.yaml`),
the artifacts built inside the container should be available as if they had been
built on the host, and with the expected user permissions (i.e. the created
files will be owned by the local user, rather than `root`). Dedicated cache
volumes and other utilities are also available in order to speed up the
development loop.

See <https://seankelleher.ie/tags/docker/> for a series of articles discussing
the ideas captured by this project.

Overview
--------

* `dock run-in` runs once-off commands in a given container environment, and is
  generally expected to be run in a CI (non-interactive) environment.
* `dock shell` spawns a shell in a given container environment, and is generally
  expected to be run locally (in an interactive environment).
* `dock rebuild` is intended to be used like `docker build`, but removes the old
  image associated with the given tag if the build is successful.

Installation
------------

The simplest way to install Dock at present is to build a release version of
this project using the steps outlined in the "Release" section, and then add it
to your `PATH`:

    ARCH='x86_64-unknown-linux-musl'
    bash scripts/build_release.sh "$ARCH"
    sudo install target/"$ARCH"/release/dock /usr/local/bin

Usage
-----

### `dock`

`dock` runs the `shell` subcommand by default if no subcommand is given.

### `dock rebuild`

`dock rebuild` takes the same arguments as `docker build`, but requires the
first argument to be an image name:

    dock rebuild ezanmoto/dock.build:v123 -f build.Dockerfile .
    dock rebuild ezanmoto/dock.build:v123 - < build.Dockerfile

This "rebuilds" the image by building a new image with the given name, and
removing the old image with the same name, if any. This can allow developers to
more easily build images repeatedly without leaving unnamed images behind. See
[Docker Build `--replace`](https://seankelleher.ie/posts/docker_rbuild/) for
more details on this concept.

#### Container removal

By default, `docker build` removes intermediate containers after a successful
build, but leaves them after a failed build. `dock rebuild` is intended to be
used for repeated rebuilds of Docker images, without leaving unused images and
containers behind. As such, the default behaviour of `dock rebuild` is to always
remove intermediate containers regardless of the build result.

### `dock run-in`

`dock run-in` runs a shell command in a Docker "environment". For example,
consider the following definition of a project whose build environment is
defined in a separate `build.Dockerfile`, and which also has the following
`dock.yaml`:

``` yaml
schema_version: '0.1'
organisation: 'ezanmoto'
project: 'dock'
default_shell_env: 'build'

environments:
  build: {}
```

Running `dock run-in build make` or `dock run-in build-env: make` (see "Extended
environment flag", below) will do the following, in order:

1. Rebuilds the Docker image `ezanmoto/dock.build` from a local
   `build.Dockerfile`, if it needs to be rebuilt.
2. Runs `make` in a new container created from `ezanmoto/dock.build`.

The container is run with `--rm`, so it is automatically removed after the
command finishes.

#### Configuration

Extra parameters can be provided to the underlying `docker run` command using
the environment block. All fields in the environment block are optional.

``` yaml
schema_version: '0.1'
organisation: 'ezanmoto'
project: 'dock'
default_shell_env: 'build'

environments:
  build:
    workdir: /app

    build_args:
    - --build-arg=USER_ID=1000

    run_args:
    - --env=XDG_CACHE_HOME=/tmp/cache
    - --group-add=test

    env:
      PROXY_FORWARDING: true

    mount_local:
    - user
    - group
    - project_dir
    - docker

    cache_volumes:
      tmp: /tmp/cache
      pkg: /go/pkg

    # NOTE `mounts` work with "nested" Docker instances; see below for more
    # details.
    mounts:
      ./relative/path: /inner/path

    shell: /bin/bash
```

* `default_shell_env`: This is the environment that `dock shell` will spawn a
  shell in if no environment is provided.
* `workdir`: This defines the directory that the command is run in inside the
  container.
* `build_args`: These arguments are passed to the underlying `docker build`
  command in the same order.
* `run_args`: These arguments are passed to the underlying `docker run` command
  in the same order.
* `env`: These environment variable definitions are exported inside the Docker
  container.
* `mount_local.user`: This performs "local user mapping", so that the command
  run inside the container is run with the user ID of the user running `dock`.
  Note that this ID is discovered using the `id` program, and so, a failure may
  occur if the `id` program isn't found.
* `mount_local.group`: This is similar to `mount_local.user`, but uses the local
  user's group ID instead of their user ID. It requires that `mount_local.user`
  is also specified.
* `mount_local.project_dir`: This mounts the local project directory, i.e. the
  directory that `dock.yaml` is defined in, to the `workdir` path inside the
  container. This also works in "nested" Docker scenarios, as described in the
  "`mounts`" section, below.
* `mount_local.docker`: This mounts the default local Docker socket file inside
  the container and, inside the container, adds the user to the owner group for
  the host socket file.
* `cache_volumes`: This creates a new volume at the given path, but recursively
  changes the permissions of the path to have open (`0777`) permissions. See the
  "`cache_volumes`" section, below, for more details.
* `mounts`: This section defines bind mounts, where the source paths are
  relative to the directory containing `dock.yaml` (as opposed to being defined
  using absolute paths). These can also allow for bind mounts in "nested" Docker
  scenarios, where the Docker server is made available to a container by
  enabling `nested_docker`. See the "`mounts`" section, below, for more details.
* `shell`: This defines the shell to run for `dock shell`.

##### `mounts`

The `mounts` section provides a shortcut for bind-mounting files and directories
that are defined relative to `dock.yaml`. In addition, "nested" bind mounts can
be made possible with this approach, as described in the rest of this section.

With the regular operation of Docker, bind mounts are defined such that the
source path is an absolute path on the Docker host, which is generally
sufficient when the container is being run "directly" on the host. However, a
container may be run in a "nested" context. For example, a CI system may define
its build agent as a Docker container, which may want to run further Docker
containers. A recommended approach to this nested Docker (or "Docker-in-Docker")
setup is to, instead of installing a nested Docker server, [bind-mount the
socket of the Docker server running on the
host](https://jpetazzo.github.io/2015/09/03/do-not-use-docker-in-docker-for-ci/#the-socket-solution).

One caveat of this approach is when using bind mounts for simplifying the Docker
build process, as described in [Docker for the Build
Process](https://seankelleher.ie/posts/docker_for_building/). In this scenario,
absolute paths in the build agent container don't map to absolute paths on the
host environment, and so, bind mounts can't be used for the innermost
containers.

`dock` defines a `DOCK_HOSTNAMES` environment variable to track what
bind-mounts are available in a container, and can use this to map paths from
inside containers, back to the actual paths on the host. This can allow
bind-mounting to be utilised to any depth of container nesting, as long as all
paths are reachable on the host.

##### `cache_mounts`

`cache_mounts` exists to help in scenarios where a volume should be available to
a container, but where the container is run with a non-`root` user. Before a
container is run by `dock`, `dock` recursively updates the permissions of volume
directories to be open (`0777`), so that they can be written to by non-`root`
users.

The reason for this functionality is because, by default, volumes created by
`docker` are owned by root, and so can't generally be written to by non-`root`
users. Solving this can be tricky when wanting to run a container with a
non-`root` user, because changing the directory permissions requires `root`
permissions. Switching between users is possible during a `docker build`, but
it's generally not recommended to mount volumes during `docker build`, so the
solution ideally happens after this. Different options are possible, such as
using `sudo`, `su`, scripts with sticky bits, or possibly using BuildKit, but
`cache_mounts` can be used as a general, image-independent mechanism to handle
this scenario.

#### Flags

* `--debug`/`-D`: This will cause `dock run-in` to output the Docker commands
  that `dock run-in` runs, as well as the output of those commands, as they're
  being run. This can be useful, for example, to see the output of the "rebuild"
  step as it happens, as this step is usually hidden unless an error occurs.
* `--skip-rebuild`/`-R`: This will skip the rebuild step that otherwise
  happens before the command is run.
* `--tty`/`-T`: This will allocate a pseudo-TTY (PTY) for the container, so the
  command should behave as if it's running interactively.

#### Default flags

Behind the scenes, `dock run-in` passes the following to `docker run` by
default:

* `--rm`: This ensures that containers are destroyed once they finish executing.
* `--init`: This "ensures the usual responsibilities of an init system, such as
  reaping zombie processes, are performed inside the created container."

#### Extended environment flag

`dock run-in` allows providing the name of the environment with an `-env:`
suffix. This is somewhat unconventional and may be considered inconsistent with
the established conventions for the layout of UNIX commands and their arguments.
Furthermore, it goes against the Pythonic principle that is adopted by many
developers of having "one way to do it." While all of this is true, the
practicality of the approach is that it may improve the readability of `dock`
usage in scripts. Consider the following:

    dock run-in build cargo build
    dock run-in test.build sh -c 'npm i && npm run build'

While this on its own may not be considered too unreadable, the readability may
be improved with the `-env:` suffix, which highlights `test.build` as the name
of an environment, and delineates between the "context" of the command (the part
before the `:`), and the command to be run in the containerised environment:

    dock run-in build-env: cargo build
    dock run-in test.build-env: sh -c 'npm i && npm run build'

Note that other approaches, like the use of line splitting in shell scripts, may
be used to achieve equivalent results using more conventional means:

    dock run-in build \
        cargo build

    dock run-in test.build \
        sh -c 'npm i && npm run build'

### `dock shell`

`dock shell` has the same behaviour as `dock run-in`, but instead of running a
single command, spawns a new shell in the Docker "environment". If a `dock.yaml`
file contains an environment called `build`, then `dock shell build` will start
a new shell in that environment. `dock shell` on its own will start a shell in
the `default_shell_env`.

#### Default flags

Behind the scenes, `dock shell` passes the same default flags to `docker run` as
`dock run-in`, and also passes the following:

* `--network=host`: This allows services run inside the container to be accessed
  as if they were running on the host.

Development
-----------

### Build environment

The build environment for the project is defined in `build.Dockerfile`. The
build environment can be replicated locally by following the setup defined in
the Dockerfile, or Docker can be used to mount the local directory in the build
environment by running the following:

    bash scripts/with_build_env.sh bash

### Building

The project can be built locally using `cargo build --locked`, or can be built
using Docker by running the following:

    bash scripts/with_build_env.sh cargo build --locked

### Testing

The project can be tested locally using `just check`, or the tests can be run
using Docker by running the following:

    bash scripts/with_build_env.sh just check

A subset of integration tests can be run by passing name patterns to `just`:

    just check add

The commands above will run all integration tests whose name contains "add".

### Release

Release binaries are built using `build_release.sh` with a [`cross`
target](https://github.com/rust-embedded/cross#supported-targets):

    bash scripts/build_release.sh x86_64-unknown-linux-musl
