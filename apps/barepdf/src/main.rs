#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_error;
mod application;
mod diagnostics;
mod infrastructure;
mod presentation;

use app_error::AppError;

fn main() -> Result<(), AppError> {
    diagnostics::init();
    presentation::run()
}
