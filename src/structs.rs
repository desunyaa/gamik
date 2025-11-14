#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Entity(u32);

pub struct EntityGenerator(u32);

impl EntityGenerator {
    fn default() -> Self {
        EntityGenerator(0)
    }

    fn new_entity(&mut self) -> Entity {
        self.0 += 1;
        Entity(self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Point {
    pub x: u32,
    pub y: u32,
}

pub struct PointEntityMap(rustc_hash::FxHashMap<Point, Vec<Entity>>);

impl PointEntityMap {
    fn new() -> Self {
        PointEntityMap(rustc_hash::FxHashMap::default())
    }

    fn insert(&mut self, point: Point, entity: Entity) {
        self.0.entry(point).or_insert_with(Vec::new).push(entity);
    }

    fn remove(&mut self, point: &Point, entity: Entity) {
        if let Some(entities) = self.0.get_mut(point) {
            entities.retain(|&e| e != entity);
            if entities.is_empty() {
                self.0.remove(point);
            }
        }
    }

    fn get(&self, point: &Point) -> Option<&Vec<Entity>> {
        self.0.get(point)
    }
}

pub struct EntityPointMap(rustc_hash::FxHashMap<Entity, Point>);

impl EntityPointMap {
    fn new() -> Self {
        EntityPointMap(rustc_hash::FxHashMap::default())
    }

    fn insert(&mut self, entity: Entity, point: Point) {
        self.0.insert(entity, point);
    }

    fn get(&self, entity: &Entity) -> Option<&Point> {
        self.0.get(entity)
    }

    fn remove(&mut self, entity: &Entity) -> Option<Point> {
        self.0.remove(entity)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug)]
pub enum GameEvent {
    Move {
        entity: Entity,
        direction: Direction,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityType {
    Player,
    Tree,
}

pub struct EntityTypeMap(rustc_hash::FxHashMap<Entity, EntityType>);

impl EntityTypeMap {
    fn new() -> Self {
        EntityTypeMap(rustc_hash::FxHashMap::default())
    }

    fn insert(&mut self, entity: Entity, entity_type: EntityType) {
        self.0.insert(entity, entity_type);
    }

    fn get(&self, entity: &Entity) -> Option<&EntityType> {
        self.0.get(entity)
    }
}

pub struct GameWorld {
    spatial: SpatialWorld,
    pub player: Entity,
    pub entity_types: EntityTypeMap,

    pub event_queue: Vec<GameEvent>,

    pub entity_gen: EntityGenerator,
}

pub struct SpatialWorld {
    pemap: PointEntityMap,
    epmap: EntityPointMap,
}

impl SpatialWorld {
    fn new() -> Self {
        SpatialWorld {
            pemap: PointEntityMap::new(),
            epmap: EntityPointMap::new(),
        }
    }

    fn insert_ent_at_point(&mut self, ent: Entity, point: Point) {
        self.pemap.insert(point, ent);
        self.epmap.insert(ent, point);
    }

    fn move_entity(&mut self, entity: Entity, direction: Direction) {
        if let Some(current_pos) = self.epmap.get(&entity).copied() {
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

            // Only move if position actually changed
            if new_pos != current_pos {
                self.pemap.remove(&current_pos, entity);
                self.pemap.insert(new_pos, entity);
                self.epmap.insert(entity, new_pos);
            }
        }
    }
}

impl GameWorld {
    pub fn create_test_world() -> Self {
        let mut entity_gen = EntityGenerator::default();
        let mut spatial = SpatialWorld::new();
        let mut entity_types = EntityTypeMap::new();

        // Create player at center
        let player = entity_gen.new_entity();
        spatial.insert_ent_at_point(player, Point { x: 10, y: 10 });
        entity_types.insert(player, EntityType::Player);

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
            let tree = entity_gen.new_entity();
            spatial.insert_ent_at_point(tree, pos);
            entity_types.insert(tree, EntityType::Tree);
        }

        GameWorld {
            spatial,
            player,

            event_queue: Vec::new(),

            entity_types,
            entity_gen,
        }
    }

    pub fn move_entity(&mut self, entity: Entity, direction: Direction) {
        self.spatial.move_entity(entity, direction);
    }

    pub fn get_display_char(&self, point: &Point) -> &str {
        if let Some(entities) = self.spatial.pemap.get(point) {
            // Display the first entity at this position
            if let Some(&entity) = entities.first() {
                return match self.entity_types.get(&entity) {
                    Some(EntityType::Player) => "@",
                    Some(EntityType::Tree) => "木",
                    None => "?",
                };
            }
        }
        ","
    }
}
