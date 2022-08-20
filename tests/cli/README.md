README
======

About
-----

This directory contains integration tests to exercise the project's program, by
setting up a specified directory structure, running the program, and verifying
the outcomes.

This document provides some notes about the approach taken in these tests.

Unique Image IDs
----------------

All tests define `Dockerfile`s which perform `RUN echo <test-name>` as their
first step. This ensures that the new image, from that point on, will have a
unique ID. There are a couple of reasons why this is of benefit:

* Some `Dockerfile`s may not run any subsequent commands. Without running extra
  commands, an image will have the same ID as the image it's based on.
* Ensuring that each image has a unique ID makes it easier to identify which
  image a given container was created from, which can help with debugging.

Furthermore, `echo`ing the unique test name in a layer prevents that layer from
being cached and reused in other tests.

Test Files
----------

Many of the tests contain a test file file called `test.txt` in the image, which
contains the test name as content. This is generally for the purpose of
verifying that a container is being created from the correct image.

Test Base Image
---------------

These tests are intended to be be passed the name of a "unique" base image that
all test images will be based on. It is assumed that no containers, other than
those created during testing, will exist from this base image. This is so that a
check for containers descended from the base image can be performed after all
tests have completed. The default behaviour of `dock` is to clean up all
containers after they've run, whether they've been run as part of a `rebuild` or
other command, so if any containers descended from the base image exist after
testing, then this indicates that `dock` didn't clean up after all operations.

Command Error Messages
----------------------

Some tests verify the error messages returned by certain commands such as `cat`
and `touch`. The exact error messages that get generated depend on the specific
implementation of the programs used, and generally depend on the image that the
command is run in (because the image defines what implementation is installed).
As such, if different (base) images are used in these tests, the expected
messages may need to change.

`defer!`
--------

We use `defer!` to run cleanup for tests. We would generally use higher-order
functions to perform such cleanups in non-test code, but these are less
applicable in the case of tests, where test failures are triggered using panics.
As such, we opt to use `defer!` so that the cleanup code will run regardless of
how the test exited.
