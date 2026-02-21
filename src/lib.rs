#![warn(clippy::all, rust_2018_idioms)]

pub mod ecs;
pub mod fov;
pub mod game;
pub mod net;
pub mod ui;

mod app;
pub use app::GamikApp;
