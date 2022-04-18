`dock`
======

About
-----

`dock` defines a number of sub-commands to help use Docker containers as
environments.

An example of this is to define a Docker container to use as a build
environment. When implemented, `dock env build <command>` can be used to run
`command` in the context of a build environment defined as a Docker container,
but with quality-of-life shortcuts such as easy mounting of the local project
and using the local user's ID within the container in order to speed up the
development loop. See <https://seankelleher.ie/tags/docker/> for a series of
articles discussing the ideas captured by this project.

Usage
-----

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

### `dock run`

`dock run` runs a shell command in a Docker "environment". For example, consider
the following definition of a project whose build environment is defined in a
separate `build.Dockerfile`, and which also has the following `dock.yaml:

``` yaml
organisation: 'ezanmoto'
project: 'dock'

environments:
  build: {}
```

Running `dock run build make` will do the following, in order:

1. Rebuilds the Docker image `ezanmoto/dock.build` from a local
   `build.Dockerfile`, if it needs to be rebuilt.
2. Runs `make` in a new container created from `ezanmoto/dock.build`.

The container is run with `--rm`, so it is automatically removed after the
command finishes.

#### Configuration

Extra parameters can be provided to the underlying `docker run` command using
the environment block:

``` yaml
organisation: 'ezanmoto'
project: 'dock'

environments:
  build:
    args:
    - --env=PORT=8080
    - --group-add=test

    enabled:
    - local_user_group
    - nested_docker

    # NOTE `mounts` work with "nested" Docker instances; see below for more
    # details.
    mounts:
      ./relative/path: /inner/path
```

* `args`: These `args` are passed to the underlying `docker run` command in the
  same order.
* `nested_docker`: This mounts the default local Docker socket file inside the
  container and, inside the container, adds the user to the owner group for the
  socket file.
* `local_user_group`: This performs "local user mapping", so that the command
  run inside the container is run with the user ID and group ID of the user
  running `dock`. Note that these IDs are discovered using the `id` program, and
  so, a failure may occur if the `id` program isn't found.
* `mounts`: This section defines bind mounts, where the source paths are
  relative to the directory containing `dock.yaml` (as opposed to being defined
  using absolute paths). These can also allow for bind mounts in "nested" Docker
  scenarios, where the Docker server is made available to a container by
  enabling `nested_docker`. See the "`mounts`" section, below, for more details.

### `mounts`

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

Development
-----------

### Build environment

The build environment for the project is defined in `build.Dockerfile`. The
build environment can be replicated locally by following the setup defined in
the Dockerfile, or Docker can be used to mount the local directory in the build
environment by running the following:

    bash scripts/with_build_env.sh bash

### Building

The project can be built locally using `cargo build`, or can be built using
Docker by running the following:

    bash scripts/with_build_env.sh cargo build

### Testing

The project can be tested locally using `make check`, or the tests can be run
using Docker by running the following:

    bash scripts/with_build_env.sh make check

A subset of integration tests can be run by passing `TESTS` to Make:

    make check_intg TESTS=add

The command above will run all integration tests whose name contains "add".
