// Copyright 2022 Sean Kelleher. All rights reserved.
// Use of this source code is governed by an MIT
// licence that can be found in the LICENCE file.

pub trait OptionResultExt<T> {
    fn and_maybe_then<U, F, E>(self, f: F) -> Result<Option<U>, E>
    where
        F: FnOnce(T) -> Result<U, E>;
}

impl<T> OptionResultExt<T> for Option<T> {
    fn and_maybe_then<U, F, E>(self, f: F) -> Result<Option<U>, E>
    where
        F: FnOnce(T) -> Result<U, E>,
    {
        if let Some(value) = self {
            match f(value) {
                Ok(v) => Ok(Some(v)),
                Err(e) => Err(e),
            }
        } else {
            Ok(None)
        }
    }
}
