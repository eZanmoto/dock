Conventions
===========

About
-----

This document outlines general conventions used throughout this codebase.

Rules
-----

### Handler functions

There are times where functions can grow long and there may not be a good
opportunity to split the function into logical sub-abstractions, perhaps due to
time constraints or due to the limitations of existing abstractions. In such
cases the function can be split into further functions that are tightly-coupled
with the caller, either in terms of implicitly shared knowledge, leaked
abstractions, or other forms of coupling. In such situations we consider the
caller to "delegate" to the callee, meaning that the callee isn't a
general-purpose, reusable function, but is more like an extension of the caller
function. Such a callee is given the prefix `handle_` as a convention, to
indicate that it is delegated to by a specific caller, and that it shouldn't be
considered to be reusable.
