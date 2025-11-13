use egui::{FontId, RichText};

pub struct TemplateApp {
    grid_cols: usize,
    grid_rows: usize,
    button_size: Option<f32>,
    world: GameWorld,
    event_queue: Vec<GameEvent>,
    font_size: f32,
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            grid_cols: 1,      // Will be recalculated
            grid_rows: 1,      // Will be recalculated
            button_size: None, // Will be calculated on first frame
            world: GameWorld::create_test_world(),
            event_queue: Vec::new(),
            font_size: 20.0, // Default font size
        }
    }
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Include font files at compile time
        const SC_FONT_DATA: &[u8] = include_bytes!("../assets/fonts/noto-sc-regular.ttf");
        const TC_FONT_DATA: &[u8] = include_bytes!("../assets/fonts/noto-tc-regular.ttf");

        // Load Chinese font
        let mut fonts = egui::FontDefinitions::default();

        fonts.font_data.insert(
            "sc_font".to_owned(),
            std::sync::Arc::new(egui::FontData::from_static(SC_FONT_DATA)),
        );
        fonts.font_data.insert(
            "tc_font".to_owned(),
            std::sync::Arc::new(egui::FontData::from_static(TC_FONT_DATA)),
        );

        // Put sc_font first (highest priority):
        fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(0, "sc_font".to_owned());

        // Add tc_font as well
        fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(1, "tc_font".to_owned());

        // Set fonts for monospace text
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "sc_font".to_owned());

        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(1, "tc_font".to_owned());

        // Apply the fonts to the context
        cc.egui_ctx.set_fonts(fonts);

        Default::default()
    }

    fn process_events(&mut self) {
        let events: Vec<GameEvent> = self.event_queue.drain(..).collect();

        for event in events {
            match event {
                GameEvent::Move { entity, direction } => {
                    self.world.move_entity(entity, direction);
                }
            }
        }
    }
}

impl eframe::App for TemplateApp {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle keyboard input
        ctx.input(|i| {
            if i.key_pressed(egui::Key::W) || i.key_pressed(egui::Key::ArrowUp) {
                self.event_queue.push(GameEvent::Move {
                    entity: self.world.player,
                    direction: Direction::Up,
                });
            }
            if i.key_pressed(egui::Key::S) || i.key_pressed(egui::Key::ArrowDown) {
                self.event_queue.push(GameEvent::Move {
                    entity: self.world.player,
                    direction: Direction::Down,
                });
            }
            if i.key_pressed(egui::Key::A) || i.key_pressed(egui::Key::ArrowLeft) {
                self.event_queue.push(GameEvent::Move {
                    entity: self.world.player,
                    direction: Direction::Left,
                });
            }
            if i.key_pressed(egui::Key::D) || i.key_pressed(egui::Key::ArrowRight) {
                self.event_queue.push(GameEvent::Move {
                    entity: self.world.player,
                    direction: Direction::Right,
                });
            }
        });

        // Process all events
        self.process_events();

        // Right sidebar
        egui::SidePanel::right("right_panel")
            .resizable(true)
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.heading("Right Sidebar");
                ui.separator();
                ui.label("Sidebar content here");
            });

        // Bottom bar
        egui::TopBottomPanel::bottom("bottom_panel")
            .default_height(100.0)
            .show(ctx, |ui| {
                ui.heading("Bottom Bar");
                ui.separator();
                ui.label("Bottom bar content here");
            });

        // Central panel with letter grid
        egui::CentralPanel::default().show(ctx, |ui| {
            // Customize button styling for tighter spacing
            let mut style = ui.style_mut();
            style.spacing.button_padding = egui::vec2(0.0, 0.0);
            style.visuals.widgets.inactive.bg_stroke.width = 0.0;
            style.visuals.widgets.hovered.bg_stroke.width = 0.0;
            style.visuals.widgets.active.bg_stroke.width = 0.0;

            // Calculate button size on first frame if not already calculated
            if self.button_size.is_none() {
                let chinese_char = "中";
                let font_id = egui::FontId::new(self.font_size, egui::FontFamily::Proportional);
                let letter_galley = ui.fonts_mut(|f| {
                    f.layout_no_wrap(
                        chinese_char.to_string(),
                        font_id.clone(),
                        egui::Color32::WHITE,
                    )
                });

                // Get letter dimensions - use the larger dimension to make square buttons
                let letter_width = letter_galley.size().x;
                let letter_height = letter_galley.size().y;
                let letter_size = letter_width.max(letter_height);

                // Minimal padding for tight roguelike feel
                let padding = 0.0;
                self.button_size = Some(letter_size + padding);
            }

            let button_size = self.button_size.unwrap();

            // Chinese character to display
            let chinese_char = "中"; // "zhong" - meaning "middle" or "China"

            // Calculate available space
            let available_size = ui.available_size();

            // Calculate maximum number of buttons that can fit
            let max_cols = (available_size.x / button_size).floor() as usize;
            let max_rows = (available_size.y / button_size).floor() as usize;

            println!("max cols: {max_cols}");
            println!("max rows: {max_rows}");

            // Use all available space
            self.grid_cols = max_cols.max(1); // At least 1 column
            self.grid_rows = max_rows.max(1); // At least 1 row

            // Calculate center position
            let center_row = self.grid_rows / 2;
            let center_col = self.grid_cols / 2;

            // Set spacing to zero for the grid
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

            // Center the grid
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    // Create the grid
                    for row in 0..self.grid_rows {
                        ui.horizontal(|ui| {
                            for col in 0..self.grid_cols {
                                let point = Point {
                                    x: col as u32,
                                    y: row as u32,
                                };

                                // Get the character to display at this position
                                let button_text = self.world.get_display_char(&point);

                                let button = egui::Button::new(
                                    RichText::new(button_text)
                                        .font(FontId::proportional(self.font_size)),
                                )
                                .min_size(egui::vec2(button_size, button_size))
                                .small()
                                .corner_radius(0.0);
                                if ui.add(button).clicked() {
                                    println!("Button clicked at row: {}, col: {}", row, col);
                                }
                            }
                        });
                    }
                });
            });
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Entity(u32);

struct EntityGenerator(u32);

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
struct Point {
    x: u32,
    y: u32,
}

struct PointEntityMap(rustc_hash::FxHashMap<Point, Vec<Entity>>);

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

struct EntityPointMap(rustc_hash::FxHashMap<Entity, Point>);

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
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug)]
enum GameEvent {
    Move {
        entity: Entity,
        direction: Direction,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntityType {
    Player,
    Tree,
}

struct EntityTypeMap(rustc_hash::FxHashMap<Entity, EntityType>);

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

struct GameWorld {
    spatial: SpatialWorld,
    player: Entity,
    entity_types: EntityTypeMap,
    entity_gen: EntityGenerator,
}

struct SpatialWorld {
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
    fn create_test_world() -> Self {
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
            entity_types,
            entity_gen,
        }
    }

    fn move_entity(&mut self, entity: Entity, direction: Direction) {
        self.spatial.move_entity(entity, direction);
    }

    fn get_display_char(&self, point: &Point) -> &str {
        if let Some(entities) = self.spatial.pemap.get(point) {
            // Display the first entity at this position
            if let Some(&entity) = entities.first() {
                return match self.entity_types.get(&entity) {
                    Some(EntityType::Player) => "@",
                    Some(EntityType::Tree) => "🌲",
                    None => "?",
                };
            }
        }
        "中"
    }
}
