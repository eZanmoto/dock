// Copyright 2021-2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

extern crate assert_cmd;
#[macro_use]
extern crate assert_matches;
#[macro_use]
extern crate indoc;
#[macro_use]
extern crate maplit;
extern crate nix;
extern crate predicates;
#[macro_use]
extern crate scopeguard;
extern crate snafu;

mod assert_run;
mod cli;
mod docker;
mod line_matcher;
mod pty;
mod test_setup;
mod timeout;
