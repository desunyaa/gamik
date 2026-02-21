//! Core game logic — pure, deterministic, no `egui` or networking dependencies.
//!
//! This module contains all game state types, the [`GameAction`] enum for
//! state mutations, and the pure [`apply`] function that advances the game.

use bitcode::{Decode, Encode};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

// Re-export ECS types so existing consumers can still use `game::*`.
pub use crate::ecs::{
    Direction, Entity, EntityGenerator, EntityID, EntityMap, EntityType, Point,
};

// ---------------------------------------------------------------------------
// Actions & events
// ---------------------------------------------------------------------------

/// Every possible state-mutating action that can be applied to the game.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum GameAction {
    Move(Direction),
    SpawnPlayer(String),
    /// Networking-level: request to control an existing entity.
    SpawnAs(EntityID),
    SaveWorld,
}

/// Events emitted by [`apply`] so upper layers know what happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameEvent {
    EntityMoved {
        entity_id: EntityID,
    },
    PlayerSpawned {
        entity_id: EntityID,
    },
    /// Upper layer should map this entity to the requesting endpoint.
    SpawnAsRequested {
        entity_id: EntityID,
    },
    /// Upper layer should trigger a world save.
    SaveRequested,
}

// ---------------------------------------------------------------------------
// Game state
// ---------------------------------------------------------------------------

/// Pure, deterministic game state — no networking handles, no UI state.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct GameState {
    pub entity_gen: EntityGenerator,
    pub entities: EntityMap,
    pub world_name: String,
}

impl GameState {
    /// Create a test world populated with a few trees.
    pub fn create_test_world(name: String) -> Self {
        let mut entity_gen = EntityGenerator::default();
        let mut entities = EntityMap::default();

        let tree_positions = [
            Point { x: 5, y: 5 },
            Point { x: 15, y: 5 },
            Point { x: 5, y: 15 },
            Point { x: 15, y: 15 },
            Point { x: 10, y: 5 },
            Point { x: 10, y: 15 },
        ];

        for pos in tree_positions {
            let id = entity_gen.next_id();
            entities.insert(
                id,
                Entity {
                    name: None,
                    position: pos,
                    entity_type: EntityType::Tree,
                },
            );
        }

        Self {
            entity_gen,
            entities,
            world_name: name,
        }
    }

    /// Return IDs of all player-type entities.
    pub fn get_playable_entities(&self) -> Vec<EntityID> {
        self.entities
            .iter()
            .filter(|(_, e)| e.entity_type == EntityType::Player)
            .map(|(eid, _)| *eid)
            .collect()
    }

    /// Generate a forest world with randomly scattered trees.
    ///
    /// * `width` × `height` — map dimensions (tiles are at 0..width, 0..height)
    /// * `density` — probability [0.0, 1.0] that any given tile contains a tree
    /// * `seed` — deterministic seed for reproducibility
    ///
    /// Player spawn point `(width/2, height/2)` and a small area around it are
    /// kept clear of trees.
    pub fn create_forest_world(name: String, width: i32, height: i32, density: f64, seed: u64) -> Self {
        let mut entity_gen = EntityGenerator::default();
        let mut entities = EntityMap::default();

        let spawn = Point {
            x: width / 2,
            y: height / 2,
        };
        let clear_radius = 3;

        // Simple deterministic hash-based PRNG for tree placement.
        for y in 0..height {
            for x in 0..width {
                // Keep spawn area clear.
                let dx = (x - spawn.x).abs();
                let dy = (y - spawn.y).abs();
                if dx <= clear_radius && dy <= clear_radius {
                    continue;
                }

                // Deterministic pseudo-random: hash (seed, x, y).
                let hash = simple_hash(seed, x, y);
                let threshold = (density * f64::from(u32::MAX)) as u64;
                if (hash & 0xFFFF_FFFF) < threshold {
                    let id = entity_gen.next_id();
                    entities.insert(
                        id,
                        Entity {
                            name: None,
                            position: Point { x, y },
                            entity_type: EntityType::Tree,
                        },
                    );
                }
            }
        }

        Self {
            entity_gen,
            entities,
            world_name: name,
        }
    }
}

// ---------------------------------------------------------------------------
// Pure apply function
// ---------------------------------------------------------------------------

/// Apply a single [`GameAction`] to the game state and return resulting events.
///
/// This is the **only** way game state should be mutated. The function is pure:
/// given identical `(state, entity_id, action)` inputs it always produces the
/// same output, which makes it straightforward to test and to replay.
pub fn apply(state: &mut GameState, entity_id: EntityID, action: &GameAction) -> Vec<GameEvent> {
    match action {
        GameAction::Move(direction) => {
            move_entity(state, entity_id, *direction);
            vec![GameEvent::EntityMoved { entity_id }]
        }
        GameAction::SpawnPlayer(name) => {
            let new_id = spawn_player(state, name.clone());
            vec![GameEvent::PlayerSpawned { entity_id: new_id }]
        }
        GameAction::SpawnAs(eid) => {
            vec![GameEvent::SpawnAsRequested { entity_id: *eid }]
        }
        GameAction::SaveWorld => {
            vec![GameEvent::SaveRequested]
        }
    }
}

/// Spawn a new player entity and return its ID.
pub fn spawn_player(state: &mut GameState, name: String) -> EntityID {
    let id = state.entity_gen.next_id();
    state.entities.insert(
        id,
        Entity {
            name: Some(name),
            position: Point { x: 10, y: 10 },
            entity_type: EntityType::Player,
        },
    );
    id
}

/// Move an entity one tile in the given direction.
///
/// The move is rejected if the destination tile is occupied by an entity that
/// blocks movement (e.g. a tree).
pub fn move_entity(state: &mut GameState, entity_id: EntityID, direction: Direction) {
    if let Some(entity) = state.entities.get(&entity_id) {
        let (dx, dy) = direction.delta();
        let new_pos = Point {
            x: entity.position.x.saturating_add(dx),
            y: entity.position.y.saturating_add(dy),
        };

        // Check if any entity at the destination blocks movement.
        let blocked = state
            .entities
            .values()
            .any(|e| e.position == new_pos && e.entity_type.blocks_movement());

        if !blocked {
            if let Some(entity) = state.entities.get_mut(&entity_id) {
                entity.position = new_pos;
            }
        }
    }
}

/// Simple deterministic hash for world generation.
///
/// Produces a pseudo-random u64 from a seed and grid coordinates.
fn simple_hash(seed: u64, x: i32, y: i32) -> u64 {
    let mut h = seed;
    h = h.wrapping_add(x as u64);
    h ^= h << 13;
    h ^= h >> 7;
    h = h.wrapping_add(y as u64).wrapping_mul(0x517c_c1b7_2722_0a95);
    h ^= h >> 17;
    h = h.wrapping_mul(0x6c62_e91d_b73b_840b);
    h ^= h >> 31;
    h
}

// ---------------------------------------------------------------------------
// Persistence (serialization + file I/O)
// ---------------------------------------------------------------------------

/// Saves the [`GameState`] to a `.world` file in the `worlds` directory.
pub fn save_to_file(state: &GameState) -> io::Result<()> {
    let worlds_dir = PathBuf::from("worlds");
    fs::create_dir_all(&worlds_dir)?;

    let file_path = worlds_dir.join(format!("{}.world", state.world_name));
    let encoded = bitcode::encode(state);
    fs::write(&file_path, encoded)?;

    Ok(())
}

/// Loads a [`GameState`] from a `.world` file.
pub fn load_from_file(file_path: &Path) -> io::Result<GameState> {
    let bytes = fs::read(file_path)?;
    let state: GameState =
        bitcode::decode(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_state() -> GameState {
        GameState {
            entity_gen: EntityGenerator::default(),
            entities: EntityMap::default(),
            world_name: "test".into(),
        }
    }

    // -- spawn_player --------------------------------------------------------

    #[test]
    fn spawn_player_creates_entity() {
        let mut state = empty_state();
        let id = spawn_player(&mut state, "Alice".into());

        assert!(state.entities.contains_key(&id));
        let entity = &state.entities[&id];
        assert_eq!(entity.name, Some("Alice".into()));
        assert_eq!(entity.entity_type, EntityType::Player);
        assert_eq!(entity.position, Point { x: 10, y: 10 });
    }

    #[test]
    fn spawn_player_ids_are_unique() {
        let mut state = empty_state();
        let id1 = spawn_player(&mut state, "Alice".into());
        let id2 = spawn_player(&mut state, "Bob".into());
        assert_ne!(id1, id2);
    }

    // -- move_entity ---------------------------------------------------------

    #[test]
    fn move_entity_up() {
        let mut state = empty_state();
        let id = spawn_player(&mut state, "P".into());
        let start = state.entities[&id].position;

        move_entity(&mut state, id, Direction::Up);
        assert_eq!(
            state.entities[&id].position,
            Point {
                x: start.x,
                y: start.y - 1
            }
        );
    }

    #[test]
    fn move_entity_down() {
        let mut state = empty_state();
        let id = spawn_player(&mut state, "P".into());
        let start = state.entities[&id].position;

        move_entity(&mut state, id, Direction::Down);
        assert_eq!(
            state.entities[&id].position,
            Point {
                x: start.x,
                y: start.y + 1
            }
        );
    }

    #[test]
    fn move_entity_left() {
        let mut state = empty_state();
        let id = spawn_player(&mut state, "P".into());
        let start = state.entities[&id].position;

        move_entity(&mut state, id, Direction::Left);
        assert_eq!(
            state.entities[&id].position,
            Point {
                x: start.x - 1,
                y: start.y
            }
        );
    }

    #[test]
    fn move_entity_right() {
        let mut state = empty_state();
        let id = spawn_player(&mut state, "P".into());
        let start = state.entities[&id].position;

        move_entity(&mut state, id, Direction::Right);
        assert_eq!(
            state.entities[&id].position,
            Point {
                x: start.x + 1,
                y: start.y
            }
        );
    }

    #[test]
    fn move_nonexistent_entity_is_noop() {
        let mut state = empty_state();
        let before = state.clone();
        move_entity(&mut state, EntityID(999), Direction::Up);
        assert_eq!(state, before);
    }

    #[test]
    fn move_allows_negative_coordinates() {
        let mut state = empty_state();
        let id = spawn_player(&mut state, "P".into());
        // Place entity at origin
        state.entities.get_mut(&id).expect("just spawned").position = Point { x: 0, y: 0 };

        // i32::saturating_sub(1) allows going below zero (saturates at i32::MIN)
        move_entity(&mut state, id, Direction::Up);
        assert_eq!(state.entities[&id].position, Point { x: 0, y: -1 });

        state.entities.get_mut(&id).expect("exists").position = Point { x: 0, y: 0 };
        move_entity(&mut state, id, Direction::Left);
        assert_eq!(state.entities[&id].position, Point { x: -1, y: 0 });
    }

    // -- apply ---------------------------------------------------------------

    #[test]
    fn apply_move_returns_entity_moved_event() {
        let mut state = empty_state();
        let id = spawn_player(&mut state, "P".into());
        let events = apply(&mut state, id, &GameAction::Move(Direction::Right));
        assert_eq!(events, vec![GameEvent::EntityMoved { entity_id: id }]);
    }

    #[test]
    fn apply_spawn_player_returns_player_spawned_event() {
        let mut state = empty_state();
        let events = apply(
            &mut state,
            EntityID(0),
            &GameAction::SpawnPlayer("Bob".into()),
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            GameEvent::PlayerSpawned { entity_id } => {
                assert!(state.entities.contains_key(entity_id));
            }
            other => panic!("expected PlayerSpawned, got {other:?}"),
        }
    }

    #[test]
    fn apply_save_world_returns_save_requested() {
        let mut state = empty_state();
        let events = apply(&mut state, EntityID(0), &GameAction::SaveWorld);
        assert_eq!(events, vec![GameEvent::SaveRequested]);
    }

    #[test]
    fn apply_spawn_as_returns_spawn_as_requested() {
        let mut state = empty_state();
        let events = apply(&mut state, EntityID(0), &GameAction::SpawnAs(EntityID(42)));
        assert_eq!(
            events,
            vec![GameEvent::SpawnAsRequested {
                entity_id: EntityID(42)
            }]
        );
    }

    // -- determinism ---------------------------------------------------------

    #[test]
    fn identical_action_sequences_produce_identical_states() {
        let actions = vec![
            (EntityID(0), GameAction::SpawnPlayer("Alice".into())),
            (EntityID(1), GameAction::Move(Direction::Right)),
            (EntityID(1), GameAction::Move(Direction::Down)),
            (EntityID(0), GameAction::SpawnPlayer("Bob".into())),
            (EntityID(2), GameAction::Move(Direction::Left)),
        ];

        let mut state_a = empty_state();
        let mut state_b = empty_state();

        for (eid, action) in &actions {
            apply(&mut state_a, *eid, action);
            apply(&mut state_b, *eid, action);
        }

        assert_eq!(state_a, state_b);
    }

    // -- create_test_world ---------------------------------------------------

    #[test]
    fn create_test_world_has_trees() {
        let state = GameState::create_test_world("w".into());
        let tree_count = state
            .entities
            .values()
            .filter(|e| e.entity_type == EntityType::Tree)
            .count();
        assert_eq!(tree_count, 6);
    }

    // -- get_playable_entities -----------------------------------------------

    #[test]
    fn get_playable_entities_returns_only_players() {
        let mut state = GameState::create_test_world("w".into());
        assert!(state.get_playable_entities().is_empty());

        let pid = spawn_player(&mut state, "Alice".into());
        let playable = state.get_playable_entities();
        assert_eq!(playable.len(), 1);
        assert!(playable.contains(&pid));
    }

    // -- entity_type ---------------------------------------------------------

    #[test]
    fn tree_blocks_sight() {
        assert!(EntityType::Tree.blocks_sight());
    }

    #[test]
    fn player_does_not_block_sight() {
        assert!(!EntityType::Player.blocks_sight());
    }

    // -- collision -----------------------------------------------------------

    #[test]
    fn tree_blocks_movement() {
        assert!(EntityType::Tree.blocks_movement());
    }

    #[test]
    fn player_does_not_block_movement() {
        assert!(!EntityType::Player.blocks_movement());
    }

    #[test]
    fn move_blocked_by_tree() {
        let mut state = empty_state();
        let pid = spawn_player(&mut state, "P".into());
        // Place player at (5, 5).
        state
            .entities
            .get_mut(&pid)
            .expect("exists")
            .position = Point { x: 5, y: 5 };

        // Place a tree at (6, 5) — one step to the right.
        let tid = state.entity_gen.next_id();
        state.entities.insert(
            tid,
            Entity {
                name: None,
                position: Point { x: 6, y: 5 },
                entity_type: EntityType::Tree,
            },
        );

        move_entity(&mut state, pid, Direction::Right);
        // Player should NOT have moved.
        assert_eq!(
            state.entities[&pid].position,
            Point { x: 5, y: 5 },
            "player should be blocked by tree"
        );
    }

    // -- forest world gen ----------------------------------------------------

    #[test]
    fn forest_world_has_trees() {
        let state = GameState::create_forest_world("forest".into(), 50, 50, 0.15, 42);
        let tree_count = state
            .entities
            .values()
            .filter(|e| e.entity_type == EntityType::Tree)
            .count();
        assert!(
            tree_count > 0,
            "forest world should have trees"
        );
    }

    #[test]
    fn forest_world_spawn_area_clear() {
        let state = GameState::create_forest_world("forest".into(), 50, 50, 0.5, 42);
        let spawn = Point { x: 25, y: 25 };
        let clear_radius = 3;

        for e in state.entities.values() {
            if e.entity_type == EntityType::Tree {
                let dx = (e.position.x - spawn.x).abs();
                let dy = (e.position.y - spawn.y).abs();
                assert!(
                    dx > clear_radius || dy > clear_radius,
                    "tree at ({}, {}) is inside spawn clear zone",
                    e.position.x,
                    e.position.y,
                );
            }
        }
    }

    #[test]
    fn forest_world_deterministic() {
        let state_a = GameState::create_forest_world("f".into(), 30, 30, 0.2, 99);
        let state_b = GameState::create_forest_world("f".into(), 30, 30, 0.2, 99);
        assert_eq!(state_a, state_b, "same seed should produce identical worlds");
    }
}
