//! UI / rendering helpers.
//!
//! This module owns everything that depends on `egui` for presentation.
//! It reads [`GameState`](crate::game::GameState) and produces visual output —
//! no game logic lives here.

use crate::game::{Entity, EntityType, Point};
use egui::Color32;
use rustc_hash::FxHashMap;

/// Visual representation of a single grid cell.
pub struct Glyph {
    pub character: &'static str,
    pub fg_color: Color32,
    pub bg_color: Color32,
    pub size_mod: f32,
}

/// Pre-computed spatial index mapping positions to entities.
pub type SpatialIndex<'a> = FxHashMap<Point, &'a Entity>;

/// Build a spatial index from the entity map for O(1) lookups per cell.
pub fn build_spatial_index(entities: &crate::game::EntityMap) -> SpatialIndex<'_> {
    entities
        .values()
        .map(|e| (e.position, e))
        .collect()
}

/// Return the visual representation of whatever occupies `point` in the world.
pub fn glyph_at(index: &SpatialIndex<'_>, point: &Point) -> Glyph {
    if let Some(entity) = index.get(point) {
        return match entity.entity_type {
            EntityType::Player => Glyph {
                character: "@",
                fg_color: Color32::WHITE,
                bg_color: Color32::BLACK,
                size_mod: 1.0,
            },
            EntityType::Tree => Glyph {
                character: "木",
                fg_color: Color32::DARK_GREEN,
                bg_color: Color32::BLACK,
                size_mod: 1.0,
            },
        };
    }
    Glyph {
        character: ".",
        fg_color: Color32::WHITE,
        bg_color: Color32::BLACK,
        size_mod: 2.0,
    }
}
