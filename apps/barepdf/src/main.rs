#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

mod app_error;
mod application;
mod controllers;
mod diagnostics;
mod infrastructure;
mod presentation;
mod smoke_test;

use app_error::AppError;

fn main() -> Result<(), AppError> {
    if smoke_test::has_smoke_test_flag(std::env::args_os().skip(1)) {
        if let Err(error) = smoke_test::run() {
            eprintln!("BarePDF smoke test failed: {error}");
            return Err(error);
        }
        return Ok(());
    }

    diagnostics::init();
    presentation::run()
}
