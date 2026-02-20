//! UI / rendering helpers.
//!
//! This module owns everything that depends on `egui` for presentation.
//! It reads [`GameState`](crate::game::GameState) and produces visual output —
//! no game logic lives here.

use crate::game::{EntityType, GameState, Point};
use egui::Color32;

/// Visual representation of a single grid cell.
pub struct GraphicsTriple {
    pub character: &'static str,
    pub fg_color: Color32,
    pub bg_color: Color32,
    pub size_mod: f32,
}

/// Return the visual representation of whatever occupies `point` in the world.
pub fn get_graphics_triple(state: &GameState, point: &Point) -> GraphicsTriple {
    for entity in state.entities.values() {
        if entity.position == *point {
            return match &entity.entity_type {
                EntityType::Player => GraphicsTriple {
                    character: "@",
                    fg_color: Color32::WHITE,
                    bg_color: Color32::BLACK,
                    size_mod: 1.0,
                },
                EntityType::Tree => GraphicsTriple {
                    character: "木",
                    fg_color: Color32::DARK_GREEN,
                    bg_color: Color32::BLACK,
                    size_mod: 1.0,
                },
            };
        }
    }
    GraphicsTriple {
        character: ".",
        fg_color: Color32::WHITE,
        bg_color: Color32::BLACK,
        size_mod: 2.0,
    }
}
