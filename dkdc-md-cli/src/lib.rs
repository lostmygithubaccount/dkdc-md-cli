#![forbid(unsafe_code)]

pub(crate) mod auth;
mod cli;
pub(crate) mod client;

pub use cli::run;
