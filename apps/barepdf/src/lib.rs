#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![allow(dead_code, unused_imports)]

pub mod controllers;
pub mod diagnostics;
pub mod infrastructure;
