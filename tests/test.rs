// Copyright 2021-2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

extern crate assert_cmd;
#[macro_use]
extern crate indoc;
#[macro_use]
extern crate maplit;
extern crate predicates;

mod assert_run;
mod cli;
mod docker;
mod line_matcher;
mod test_setup;
