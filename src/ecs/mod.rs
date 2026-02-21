//! Entity-component storage and core value types.
//!
//! This module owns the data layout for entities and related primitives.
//! It is intentionally free of game-logic, networking, and rendering concerns.

use bitcode::{Decode, Encode};
use rustc_hash::FxHashMap;

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

/// Map from entity IDs to their data.
pub type EntityMap = FxHashMap<EntityID, Entity>;

// ---------------------------------------------------------------------------
// Core value types
// ---------------------------------------------------------------------------

/// Unique identifier for an entity in the game world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct EntityID(pub u32);

/// Monotonically increasing generator for [`EntityID`] values.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct EntityGenerator(u32);

impl EntityGenerator {
    pub fn next_id(&mut self) -> EntityID {
        self.0 += 1;
        EntityID(self.0)
    }
}

/// A 2-D point on the game grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// Cardinal direction for movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    /// Returns the `(dx, dy)` offset for one step in this direction.
    pub const fn delta(self) -> (i32, i32) {
        match self {
            Self::Up => (0, -1),
            Self::Down => (0, 1),
            Self::Left => (-1, 0),
            Self::Right => (1, 0),
        }
    }
}

/// The kind of entity.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum EntityType {
    Player,
    Tree,
}

impl EntityType {
    pub fn blocks_sight(&self) -> bool {
        matches!(self, Self::Tree)
    }

    pub fn blocks_movement(&self) -> bool {
        matches!(self, Self::Tree)
    }
}

/// An entity in the game world.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Entity {
    pub position: Point,
    pub name: Option<String>,
    pub entity_type: EntityType,
}
