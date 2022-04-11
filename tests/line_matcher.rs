// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

use std::convert::From;
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

    /// # Errors
    ///
    /// Will return `Err` if the stream has more lines.
    pub fn assert_eof(&mut self) -> Result<(), AssertEofError> {
        match self.peek() {
            None => Ok(()),
            Some(_) => Err(AssertEofError::ExpectedEof),
        }
    }

    /// # Errors
    ///
    /// Will return `Err` if the next line in the stream doesn't begin with
    /// `prefix`.
    pub fn assert_prefix<'b>(&mut self, prefix: &'b str)
        -> Result<(), AssertPrefixError<'b>>
    {
        let line =
            if let Some(ln) = self.next_line() {
                ln
            } else {
                return Err(AssertPrefixError::UnexpectedEof);
            };

        if line.starts_with(prefix) {
            Ok(())
        } else {
            Err(AssertPrefixError::UnmatchedPrefix{prefix})
        }
    }

    /// Returns the next line in the stream with `prefix` removed from the
    /// start of the string.
    ///
    /// # Errors
    ///
    /// Will return `Err` if the next line in the stream doesn't begin with
    /// `prefix`.
    pub fn assert_strip_prefix<'b>(&mut self, prefix: &'b str)
        -> Result<&str, AssertPrefixError<'b>>
    {
        let line =
            if let Some(ln) = self.next_line() {
                ln
            } else {
                return Err(AssertPrefixError::UnexpectedEof);
            };

        if let Some(remainder) = line.strip_prefix(prefix) {
            Ok(remainder)
        } else {
            Err(AssertPrefixError::UnmatchedPrefix{prefix})
        }
    }

    /// Returns `true` if the next line in the stream starts with `prefix`,
    /// and skips it.
    ///
    /// # Errors
    ///
    /// Will return `Err` if the stream has more lines.
    pub fn skip_if_starts_with<'b>(&mut self, prefix: &'b str) -> bool {
        let line =
            if let Some(ln) = self.peek() {
                ln
            } else {
                return false;
            };

        if line.starts_with(prefix) {
            self.next_line();

            return true;
        }

        false
    }

    /// # Errors
    ///
    /// Will return `Err` if the end of the stream is encountered.
    pub fn assert_skip_until_starts_with<'b>(&mut self, prefix: &'b str)
        -> Result<(), AssertPrefixError<'b>>
    {
        while self.skip_if_not_starts_with(prefix) {
        }

        // TODO This function should ideally be replaced with an equivalent
        // function that returns `bool`, because in theory the only error that
        // `assert_prefix` should return is `UnexpectedEof`.
        self.assert_prefix(prefix)
    }

    pub fn skip_if_not_starts_with<'b>(&mut self, prefix: &'b str) -> bool {
        let line =
            if let Some(ln) = self.peek() {
                ln
            } else {
                return false;
            };

        if !line.starts_with(prefix) {
            self.next_line();

            return true;
        }

        false
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
pub enum AssertEofError {
    ExpectedEof,
}

#[derive(Debug)]
pub enum AssertPrefixError<'a> {
    UnmatchedPrefix{prefix: &'a str},
    UnexpectedEof,
}

#[derive(Debug)]
pub enum AssertError<'a> {
    ExpectedEof,
    UnmatchedPrefix{prefix: &'a str},
    UnexpectedEof,
}

impl<'a> From<AssertEofError> for AssertError<'a> {
    fn from(source: AssertEofError) -> Self {
        match source {
            AssertEofError::ExpectedEof => Self::ExpectedEof,
        }
    }
}

impl<'a> From<AssertPrefixError<'a>> for AssertError<'a> {
    fn from(source: AssertPrefixError<'a>) -> Self {
        match source {
            AssertPrefixError::UnmatchedPrefix{prefix} =>
                Self::UnmatchedPrefix{prefix},
            AssertPrefixError::UnexpectedEof =>
                Self::UnexpectedEof,
        }
    }
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
        AssertError::ExpectedEof => {
            lines.push(
                body
                    .lines()
                    .nth(line_num-1)
                    .unwrap()
                    .to_string()
            );
            lines.push(render_separator_with_msg("expected eof"));
            lines.extend(
                body
                    .lines()
                    .skip(line_num)
                    .map(ToString::to_string)
            );
            lines.push(render_separator());
        },
        AssertError::UnmatchedPrefix{prefix} => {
            lines.push(render_separator_with_msg("expected prefix"));
            lines.push((*prefix).to_string());
            lines.push(render_separator_with_msg("got"));
            lines.push(
                body
                    .lines()
                    .nth(line_num-1)
                    .unwrap()
                    .to_string()
            );
            lines.push(render_separator());
        },
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
