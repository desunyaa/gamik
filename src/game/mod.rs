//! Core game logic — pure, deterministic, no `egui` or networking dependencies.
//!
//! This module contains all game state types, the [`GameAction`] enum for
//! state mutations, and the pure [`apply`] function that advances the game.

use bitcode::{Decode, Encode};
use rustc_hash::FxHashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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
    fn next(&mut self) -> EntityID {
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
}

/// An entity in the game world.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Entity {
    pub position: Point,
    pub name: Option<String>,
    pub entity_type: EntityType,
}

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
            let id = entity_gen.next();
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
    let id = state.entity_gen.next();
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
pub fn move_entity(state: &mut GameState, entity_id: EntityID, direction: Direction) {
    if let Some(entity) = state.entities.get_mut(&entity_id) {
        let (dx, dy) = direction.delta();
        entity.position.x = entity.position.x.saturating_add(dx);
        entity.position.y = entity.position.y.saturating_add(dy);
    }
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
}
