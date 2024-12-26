// Copyright 2022-2024 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::iter::Peekable;
use std::string::ToString;
use std::str::Lines;

pub struct LineMatcher<'a> {
    lines: Peekable<Lines<'a>>,
    line_num: usize,
    is_eof: bool,
}

impl<'a> LineMatcher<'a> {
    #[must_use]
    pub fn new(body: &'a str) -> Self {
        Self{
            lines: body.lines().peekable(),
            line_num: 0,
            is_eof: false,
        }
    }

    #[must_use]
    pub fn line_num(&self) -> usize {
        self.line_num
    }

    pub fn peek(&mut self) -> Option<&&str> {
        self.lines.peek()
    }

    pub fn next_line(&mut self) -> Option<&str> {
        // We track `is_eof` separately so that we only increment `line_num`
        // once when we encounter the end of the stream.
        if self.is_eof {
            return None;
        }

        let line = self.lines.next();
        self.line_num += 1;

        if line.is_none() {
            self.is_eof = true;
        }

        line
    }
}

#[derive(Debug)]
pub enum AssertError {
    UnexpectedEof,
}

#[must_use]
pub fn render_match_error(body: &str, line_num: usize, e: &AssertError)
    -> String
{
    let mut lines = vec![render_separator()];

    lines.extend(
        body
            .lines()
            .take(line_num-1)
            .map(ToString::to_string)
    );

    match e {
        AssertError::UnexpectedEof => {
            lines.push(render_separator_with_msg("unexpected eof"));
        },
    }

    format!("\n{}\n", lines.join("\n"))
}

fn render_separator() -> String {
    "=".repeat(50)
}

fn render_separator_with_msg(msg: &str) -> String {
    format!("{} {} {}", "=".repeat(3), msg, "=".repeat(45 - msg.len()))
}
