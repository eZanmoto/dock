# Copyright 2022-2024 Sean Kelleher. All rights reserved.
# Use of this source code is governed by an MIT
# licence that can be found in the LICENCE file.

# Note that `target` is used as the output directory for Rust so care should be
# taken that collisions don't occur between Rust output and local output.
tgt_dir := join(justfile_directory(), 'target')
tgt_test_dir := join(tgt_dir, 'tests')

test_org := 'ezanmoto'
test_proj := 'dock.test'
test_base_img_name := test_org + '/' + test_proj + '_base'
test_base_img_tag := 'latest'
test_vol_namespace := test_org + '.' + test_proj

# These are passed to `cargo test` and `cargo clippy` to provide parameters to
# test files.
export TEST_ORG := test_org
export TEST_PROJ := test_proj
export TEST_IMG_NAMESPACE := test_org + '/' + test_proj
export TEST_DIR := tgt_test_dir
export TEST_BASE_IMG := test_base_img_name + ':' + test_base_img_tag

# List available recipes.
default:
    just --list

build_release target='x86_64-unknown-linux-musl':
    bash scripts/build_release.sh '{{target}}'

# Run all checks.
check *tests: && check_lint
    just check_intg {{tests}}

# We run `clean_images` before starting the tests in order to make the tests
# more deterministic, because having leftover images from previous runs can
# cause the output from `docker build` to be altered (due to the use of
# caching).

# Run integration tests.
check_intg *tests: clean_images remake_test_dir
    @# We pull base Docker images required by the tests, even though they'd
    @# automatically be pulled during builds, in order to make the output more
    @# predictable.
    bash scripts/docker_rbuild.sh \
            '{{test_base_img_name}}' \
            '{{test_base_img_tag}}' \
            - \
        < 'test_base.Dockerfile'
    cargo test \
        --locked \
        -- \
        --nocapture \
        --show-output \
        {{tests}}
    @# Descendents of the test base image indicate that `dock` didn't clean up
    @# after all operations. See "Test Base Image" in `tests/cli/README.md` for
    @# more information.
    bash scripts/check_no_descendents.sh "$TEST_BASE_IMG"

# Run linters.
check_lint:
    cargo clippy \
        --locked \
        --all-targets \
        --all-features \
        -- \
        -D warnings \
        -D clippy::pedantic \
        -D clippy::cargo \
        -A clippy::module-name-repetitions
    python3 scripts/check_line_length.py \
        '**/*.rs' \
        79

# Remove and create the test directory.
remake_test_dir:
    mkdir --parents '{{tgt_dir}}'
    rm -rf '{{tgt_test_dir}}'
    mkdir '{{tgt_test_dir}}'

# Remove all test artefacts created in Docker.
clean: clean_images clean_volumes

# Remove all test images.
clean_images:
    bash scripts/clean_images.sh "$TEST_IMG_NAMESPACE"
    # TODO Remove hard-coded namespace.
    bash scripts/clean_images.sh "org/proj"

# Remove all test volumes.
clean_volumes:
    bash scripts/clean_volumes.sh '{{test_vol_namespace}}'
