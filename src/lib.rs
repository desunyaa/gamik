#![warn(clippy::all, rust_2018_idioms)]

mod human;

pub mod game;
pub mod net;
pub mod ui;

mod app;
pub use app::TemplateApp;
