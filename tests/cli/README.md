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
