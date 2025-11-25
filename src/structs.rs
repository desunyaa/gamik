use bincode::{Decode, Encode};
use rustc_hash::FxHashMap;

pub type EntityMap = FxHashMap<EntityID, Entity>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct EntityID(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct EntityGenerator(u32);

impl EntityGenerator {
    fn default() -> Self {
        EntityGenerator(0)
    }

    fn new_entity(&mut self) -> EntityID {
        self.0 += 1;
        EntityID(self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, Encode, Decode)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum GameEvent {
    Move {
        entity: EntityID,
        direction: Direction,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum EntityType {
    Player,
    Tree,
}

#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct Entity {
    pub position: Point,
    pub entity_type: EntityType,
}

#[derive(Debug)]
pub struct GameWorld {
    pub player: EntityID,
    pub entity_gen: EntityGenerator,
    pub event_queue: Vec<GameEvent>,

    // entities now stored in a hashmap
    pub entities: EntityMap,
}

impl GameWorld {
    pub fn create_test_world() -> Self {
        let mut entity_gen = EntityGenerator::default();
        let mut entities = EntityMap::default();

        // Create player
        let player = entity_gen.new_entity();
        entities.insert(
            player,
            Entity {
                position: Point { x: 10, y: 10 },
                entity_type: EntityType::Player,
            },
        );

        // Create some trees
        let tree_positions = vec![
            Point { x: 5, y: 5 },
            Point { x: 15, y: 5 },
            Point { x: 5, y: 15 },
            Point { x: 15, y: 15 },
            Point { x: 10, y: 5 },
            Point { x: 10, y: 15 },
        ];

        for pos in tree_positions {
            let tree_id = entity_gen.new_entity();
            entities.insert(
                tree_id,
                Entity {
                    position: pos,
                    entity_type: EntityType::Tree,
                },
            );
        }

        GameWorld {
            player,
            entity_gen,
            event_queue: Vec::new(),
            entities,
        }
    }

    pub fn move_entity(&mut self, entity_id: EntityID, direction: Direction) {
        if let Some(entity) = self.entities.get_mut(&entity_id) {
            let current_pos = entity.position;
            let new_pos = match direction {
                Direction::Up => Point {
                    x: current_pos.x,
                    y: current_pos.y.saturating_sub(1),
                },
                Direction::Down => Point {
                    x: current_pos.x,
                    y: current_pos.y + 1,
                },
                Direction::Left => Point {
                    x: current_pos.x.saturating_sub(1),
                    y: current_pos.y,
                },
                Direction::Right => Point {
                    x: current_pos.x + 1,
                    y: current_pos.y,
                },
            };

            entity.position = new_pos;
        }
    }

    pub fn gen_client_info(&self, entity_id: EntityID) -> EntityMap {
        self.entities.clone()
    }

    pub fn process_events(&mut self) {
        let events: Vec<GameEvent> = self.event_queue.drain(..).collect();

        for event in events {
            match event {
                GameEvent::Move { entity, direction } => {
                    self.move_entity(entity, direction);
                }
            }
        }
    }

    pub fn get_display_char(&self, point: &Point) -> &str {
        for entity in self.entities.values() {
            if entity.position == *point {
                return match entity.entity_type {
                    EntityType::Player => "@",
                    EntityType::Tree => "木",
                };
            }
        }
        ","
    }
}
