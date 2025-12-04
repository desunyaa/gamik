#![warn(clippy::all, rust_2018_idioms)]
mod human;
pub use human::*;
mod app;
pub use app::TemplateApp;
mod structs;
pub use structs::*;
mod network;
pub use network::*;
