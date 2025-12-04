use crate::ServerMessage;
use crate::human::*;
use bincode::{Decode, Encode};
use egui::ahash::HashMapExt;
use iroh::EndpointId;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
pub type EntityMap = FxHashMap<EntityID, Entity>;
pub type EndpointMap = FxHashMap<EndpointId, EntityID>;

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
pub enum GameCommand {
    Move(Direction),
    SpawnPlayer(String),
    SpawnAs(EntityID),
    SaveWorld,
}

pub type GameEvent = (EntityID, GameCommand);

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum EntityType {
    Player(Human),
    Tree,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct Entity {
    pub position: Point,
    pub name: Option<String>,
    pub entity_type: EntityType,
}

#[derive(Debug)]
pub struct GameWorld {
    pub entity_gen: EntityGenerator,
    pub event_queue: Vec<GameEvent>,
    pub endpoints: EndpointMap,

    // entities now stored in a hashmap
    pub entities: EntityMap,
    pub world_name: String,
    pub unique_server_messages: FxHashMap<EndpointId, Vec<ServerMessage>>,
}

// Serializable version of GameWorld (without the non-serializable fields)
#[derive(Encode, Decode)]
pub struct SerializableGameWorld {
    pub entity_gen: EntityGenerator,
    pub entities: EntityMap,
    pub world_name: String,
}

impl GameWorld {
    pub fn spawn_player(&mut self, name: String) -> EntityID {
        let human = Human::new();

        let player = self.entity_gen.new_entity();
        self.entities.insert(
            player,
            Entity {
                name: Some(name),
                position: Point { x: 10, y: 10 },
                entity_type: EntityType::Player(human),
            },
        );

        player
    }

    /// Saves the GameWorld to a .world file in the worlds directory
    pub fn save_to_file(&self) -> io::Result<()> {
        // Create worlds directory if it doesn't exist
        let worlds_dir = PathBuf::from("worlds");
        fs::create_dir_all(&worlds_dir)?;

        // Create the full path
        let file_path = worlds_dir.join(format!("{}.world", self.world_name));

        // Create serializable version
        let serializable = SerializableGameWorld {
            entity_gen: self.entity_gen,
            entities: self.entities.clone(),
            world_name: self.world_name.clone(),
        };

        // Serialize to bytes
        let encoded = bincode::encode_to_vec(&serializable, bincode::config::standard())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        // Write to file
        fs::write(&file_path, encoded)?;

        println!("World saved to: {:?}", file_path);
        Ok(())
    }

    /// Loads a GameWorld from a .world file
    pub fn load_from_file(file_path: &Path) -> io::Result<Self> {
        // Read file bytes
        let bytes = fs::read(file_path)?;

        // Deserialize
        let (serializable, _): (SerializableGameWorld, usize) =
            bincode::decode_from_slice(&bytes, bincode::config::standard())
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        // Reconstruct GameWorld
        Ok(GameWorld {
            unique_server_messages: FxHashMap::default(),
            entity_gen: serializable.entity_gen,
            entities: serializable.entities,
            event_queue: Vec::new(),
            endpoints: EndpointMap::new(),
            world_name: serializable.world_name,
        })
    }
    pub fn create_test_world(name: String) -> Self {
        let mut entity_gen = EntityGenerator::default();
        let mut entities = EntityMap::default();

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
                    name: None,
                    position: pos,
                    entity_type: EntityType::Tree,
                },
            );
        }

        GameWorld {
            unique_server_messages: FxHashMap::default(),

            endpoints: EndpointMap::new(),
            entity_gen,
            world_name: name,
            event_queue: Vec::new(),
            entities,
        }
    }

    pub fn get_playable_entities(&self) -> Vec<EntityID> {
        let mut e_vec = Vec::new();
        for (eid, e) in self.entities.iter() {
            match e.entity_type {
                EntityType::Player(_) => {
                    e_vec.push(eid.clone());
                }
                _ => {}
            }
        }

        e_vec
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

    pub fn gen_client_info(&self) -> EntityMap {
        self.entities.clone()
    }

    pub fn process_events(&mut self) {
        let events: Vec<GameEvent> = self.event_queue.drain(..).collect();

        for (eid, command) in events {
            match command {
                GameCommand::Move(direction) => {
                    self.move_entity(eid, direction);
                }
                GameCommand::SpawnPlayer(name) => {
                    // Do nothing here this is covered in the networking code
                }
                GameCommand::SpawnAs(eid) => {
                    // Do nothing here this is covered in the networking code
                }

                GameCommand::SaveWorld => {
                    self.save_to_file();
                }
            }
        }
    }

    pub fn get_display_char(&self, point: &Point) -> &str {
        for entity in self.entities.values() {
            if entity.position == *point {
                return match &entity.entity_type {
                    EntityType::Player(human) => "@",
                    EntityType::Tree => "木",
                };
            }
        }
        ","
    }
}
