use crate::structs::*;

use crate::network::*;
use egui::{FontId, RichText};
use iroh::EndpointAddr;
use iroh::EndpointId;
use iroh::protocol::Router;
use std::fs;
use std::path::PathBuf;
use tokio::sync::mpsc;
pub struct TemplateApp {
    player_id: EntityID,
    grid_cols: usize,
    grid_rows: usize,
    button_size: Option<f32>,
    menu_input_string: String,

    world: GameWorld,
    font_size: f32,
    router: Option<Router>,
    // Networking state
    server_to_client_rx: Option<mpsc::UnboundedReceiver<Message>>,
    client_to_server_tx: Option<mpsc::UnboundedSender<GameCommand>>, // New field
    game_state: GameState,
    single_player: bool,
}
#[derive(Debug, Clone, PartialEq)]
enum GameState {
    MainMenu,
    CharacterCreation,
    WorldCreation,
    CharacterSelection,
    WorldSelection,
    Playing,
}
impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            menu_input_string: String::new(),
            router: None,
            game_state: GameState::MainMenu,
            player_id: 0,
            grid_cols: 1,
            grid_rows: 1,
            button_size: None,
            world: GameWorld::create_test_world("default".into()),
            font_size: 13.0,
            server_to_client_rx: None,
            client_to_server_tx: None, // Initialize as None
            single_player: true,
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

        let app = Self::default();

        // Start the networking

        app
    }

    fn start_client<A>(&mut self, addr: A)
    where
        A: Into<EndpointAddr> + Clone,
    {
        let (msg_tx, msg_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let s_addr = addr.clone().into();

        self.server_to_client_rx = Some(msg_rx);
        self.client_to_server_tx = Some(event_tx);

        tokio::spawn(async move {
            let _ = run_client_internal(s_addr, msg_tx, event_rx).await;
        });
    }
    fn start_server(&mut self, world: GameWorld) {
        let (router_tx, mut router_rx) = mpsc::unbounded_channel();

        // Spawn an async task to start the server
        tokio::spawn(async move {
            match run_server_internal(world).await {
                Ok(router) => {
                    println!(
                        "Server started successfully at {:#?}",
                        router.endpoint().addr()
                    );
                    // Send the router back to the main thread
                    let _ = router_tx.send(router);
                }
                Err(e) => eprintln!("Server error: {}", e),
            }
        });

        while self.router.is_none() {
            // Try to receive the router (non-blocking)
            // Note: This won't work immediately since the server starts asynchronously
            // You'll need to poll for it in the update loop
            if let Ok(router) = router_rx.try_recv() {
                self.router = Some(router);
                println!("GOT ROUTER");
            }
        }
    }
}

impl eframe::App for TemplateApp {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for new message counts from server
        if let Some(rx) = &mut self.server_to_client_rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    Message::ServerMessage(smsg) => match smsg {
                        ServerMessage::EntityMap(emap) => {
                            for (eid, e) in emap.iter().enumerate() {
                                self.world.entities.insert(eid.clone(), e.clone());
                            }
                        }
                        ServerMessage::PlayerID(pid) => self.player_id = pid,
                    },

                    _ => {}
                }
            }
        }

        // Request continuous repainting to keep UI responsive
        ctx.request_repaint();
        match self.game_state {
            GameState::MainMenu => {
                self.show_main_menu(ctx);
            }

            GameState::CharacterCreation => {
                self.show_character_creation_menu(ctx);
            }
            GameState::WorldCreation => {
                self.show_world_creation_menu(ctx);
            }
            GameState::CharacterSelection => {
                self.show_character_selection_menu(ctx);
            }
            GameState::WorldSelection => {
                self.show_world_selection_menu(ctx);
            }
            GameState::Playing => {
                // Check for new message counts from server
                if let Some(rx) = &mut self.server_to_client_rx {
                    while let Ok(smsg) = rx.try_recv() {
                        if let Message::ServerMessage(ServerMessage::EntityMap(emap)) = smsg {
                            for (eid, e) in emap.iter().enumerate() {
                                self.world.entities.insert(eid.clone(), e.clone());
                            }
                        }
                    }
                }

                // Handle keyboard input
                self.input(ctx);

                // Right sidebar
                self.right_panel(ctx);

                // Bottom bar
                self.bottom_panel(ctx);

                // Central panel with letter grid
                self.rogue_screen(ctx);
            }
        }
    }
}

impl TemplateApp {
    fn show_main_menu(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);

                ui.heading(RichText::new("Roguelike Game").size(32.0));

                ui.add_space(50.0);

                if ui.button(RichText::new("Start Game").size(20.0)).clicked() {
                    self.single_player = true;
                    self.game_state = GameState::WorldSelection;
                }

                ui.add_space(20.0);

                if ui
                    .button(RichText::new("Join Online Game").size(20.0))
                    .clicked()
                {
                    self.single_player = false;
                    // Parse the endpoint address
                    match self.menu_input_string.parse::<EndpointId>() {
                        Ok(addr) => {
                            self.start_client(addr);
                            self.menu_input_string.clear();
                            self.game_state = GameState::CharacterSelection;
                        }
                        Err(_) => {
                            self.game_state = GameState::MainMenu;
                        }
                    }
                }

                ui.add_space(20.0);

                ui.label("Enter Server ID:");
                ui.text_edit_singleline(&mut self.menu_input_string);
            });
        });
    }

    fn show_world_selection_menu(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);

                ui.heading("World Selection");
                ui.add_space(20.0);

                // Create New World button
                if ui
                    .button(RichText::new("Create New World").size(20.0))
                    .clicked()
                {
                    self.game_state = GameState::WorldCreation;
                }

                ui.add_space(30.0);

                // List existing worlds
                let world_files = get_world_files();

                if world_files.is_empty() {
                    ui.label("No existing worlds found");
                } else {
                    ui.label(RichText::new("Load Existing World:").size(16.0));
                    ui.add_space(10.0);

                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for world_path in world_files {
                                if let Some(filename) = world_path.file_stem() {
                                    if let Some(name) = filename.to_str() {
                                        if ui.button(RichText::new(name).size(18.0)).clicked() {
                                            // Load the world here
                                            if let Ok(world) =
                                                GameWorld::load_from_file(&world_path)
                                            {
                                                self.start_server(world);

                                                let eid =
                                                    self.router.as_ref().unwrap().endpoint().addr();
                                                self.start_client(eid);
                                                self.game_state = GameState::CharacterSelection;
                                            }
                                        }
                                    }
                                }
                            }
                        });
                }

                ui.add_space(20.0);

                // Back button
                if ui.button(RichText::new("Back").size(16.0)).clicked() {
                    self.game_state = GameState::MainMenu;
                }
            });
        });
    }

    fn show_character_selection_menu(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);

                ui.heading("Character Selection");
                ui.add_space(20.0);

                // Create New World button
                if ui
                    .button(RichText::new("Create New Character").size(20.0))
                    .clicked()
                {
                    self.game_state = GameState::CharacterCreation;
                }

                ui.add_space(30.0);

                let playables = self.world.get_playable_entities();

                if playables.is_empty() {
                    ui.label("No existing characters found");
                } else {
                    ui.label(RichText::new("Load Existing Character:").size(16.0));
                    ui.add_space(10.0);

                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for playable in playables {
                                if ui
                                    .button(RichText::new(format!("{:#?}", playable)).size(18.0))
                                    .clicked()
                                {
                                    if let Some(tx) = &self.client_to_server_tx {
                                        if let Err(e) = tx.send(GameCommand::SpawnAs(playable)) {
                                            eprintln!("Failed to send game event: {}", e);
                                        } else {
                                            self.game_state = GameState::Playing;
                                        }
                                    }
                                }
                            }
                        });
                }

                ui.add_space(20.0);

                // Back button
                if ui.button(RichText::new("Back").size(16.0)).clicked() {
                    if self.single_player {
                        self.game_state = GameState::WorldSelection;
                    } else {
                        self.game_state = GameState::MainMenu;
                    }
                }
            });
        });
    }
    fn show_world_creation_menu(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);

                ui.heading("World Creation");
                ui.add_space(20.0);

                // Text input for world name
                ui.label("Enter world name:");
                ui.add_space(5.0);

                // Add a local variable to store the world name input
                // You'll need to add this field to TemplateApp: world_name_input: String
                ui.text_edit_singleline(&mut self.menu_input_string);

                ui.add_space(20.0);

                // Create Test World button
                if ui
                    .button(RichText::new("Create Test World").size(20.0))
                    .clicked()
                {
                    let world_name = if self.menu_input_string.trim().is_empty() {
                        // Generate a default name with timestamp
                        format!("world_{}", "lol")
                    } else {
                        self.menu_input_string.trim().to_string()
                    };
                    self.menu_input_string.clear();
                    // Create and save the world
                    let new_world = GameWorld::create_test_world(world_name.clone());
                    match new_world.save_to_file() {
                        Ok(_) => {
                            println!("World '{}' created successfully!", world_name);
                            // Clear the input
                            self.menu_input_string.clear();
                            // Return to world selection
                            self.game_state = GameState::WorldSelection;
                        }
                        Err(e) => {
                            eprintln!("Failed to create world: {}", e);
                        }
                    }
                }

                ui.add_space(20.0);

                // Back button
                if ui.button(RichText::new("Back").size(16.0)).clicked() {
                    self.game_state = GameState::WorldSelection;
                }
            });
        });
    }

    fn show_character_creation_menu(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);

                ui.heading("Character Creation");
                ui.add_space(20.0);

                // Text input for world name
                ui.label("Enter Character name:");
                ui.add_space(5.0);

                // Add a local variable to store the world name input
                // You'll need to add this field to TemplateApp: world_name_input: String
                ui.text_edit_singleline(&mut self.menu_input_string);

                ui.add_space(20.0);

                // Create Test World button
                if ui
                    .button(RichText::new("Create New Character").size(20.0))
                    .clicked()
                {
                    let char_name = if self.menu_input_string.trim().is_empty() {
                        // Generate a default name with timestamp
                        format!("John")
                    } else {
                        self.menu_input_string.trim().to_string()
                    };
                    self.menu_input_string.clear();
                    if let Some(tx) = &self.client_to_server_tx {
                        if let Err(e) = tx.send(GameCommand::SpawnPlayer(char_name)) {
                            eprintln!("Failed to send game event: {}", e);
                        } else {
                            self.game_state = GameState::CharacterSelection;
                        }
                    }
                }

                ui.add_space(20.0);

                // Back button
                if ui.button(RichText::new("Back").size(16.0)).clicked() {
                    self.game_state = GameState::CharacterSelection;
                }
            });
        });
    }
    pub fn input(&mut self, ctx: &egui::Context) {
        let mut messages_to_send = Vec::new();

        ctx.input(|i| {
            if i.key_pressed(egui::Key::W) || i.key_pressed(egui::Key::ArrowUp) {
                messages_to_send.push(GameCommand::Move(Direction::Up));
            }

            if i.key_pressed(egui::Key::Q) {
                panic!();
            }

            if i.key_pressed(egui::Key::S) || i.key_pressed(egui::Key::ArrowDown) {
                messages_to_send.push(GameCommand::Move(Direction::Down));
            }
            if i.key_pressed(egui::Key::A) || i.key_pressed(egui::Key::ArrowLeft) {
                messages_to_send.push(GameCommand::Move(Direction::Left));
            }
            if i.key_pressed(egui::Key::D) || i.key_pressed(egui::Key::ArrowRight) {
                messages_to_send.push(GameCommand::Move(Direction::Right));
            }
            if i.key_pressed(egui::Key::R) {
                messages_to_send.push(GameCommand::SaveWorld);
            }
        }); // Send all the collected messages
        if let Some(tx) = &self.client_to_server_tx {
            for event in messages_to_send {
                if let Err(e) = tx.send(event) {
                    eprintln!("Failed to send game event: {}", e);
                }
            }
        }
    }

    pub fn right_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("right_panel")
            .resizable(true)
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.heading("Right Sidebar");
                ui.separator();
                ui.label("Sidebar content here");
            });
    }

    pub fn bottom_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("bottom_panel")
            .default_height(100.0)
            .max_height(200.0)
            .show(ctx, |ui| {
                ui.heading("Bottom Bar");
                ui.separator();
                ui.label(format!("player_id: {:#?}", self.player_id));
            });
    }

    fn rogue_screen(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Customize button styling for tighter spacing
            let style = ui.style_mut();
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
                let padding = -4.0;
                self.button_size = Some(letter_size + padding);
            }

            let button_size = self.button_size.unwrap();

            // Calculate available space (use max_rect instead of available_size for accuracy)
            let available_rect = ui.max_rect();
            let available_width = available_rect.width();
            let available_height = available_rect.height();

            // Calculate maximum number of buttons that can fit
            let max_cols = ((available_width) / button_size).floor() as usize;
            let max_rows = ((available_height) / (button_size * 1.2)).floor() as usize;

            // Use all available space
            self.grid_cols = max_cols.max(1);
            self.grid_rows = max_rows.max(1);

            let player_entity = &self.world.entities[self.player_id];

            // Get player position for camera centering
            let camera_center = player_entity.position.clone();

            // Calculate camera offset to center player on screen
            let camera_offset_x = camera_center.x - (self.grid_cols as i32 / 2);
            let camera_offset_y = camera_center.y - (self.grid_rows as i32 / 2);

            // Set spacing to zero for the grid
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

            // Center the grid
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    // Create the grid
                    for row in 0..self.grid_rows {
                        ui.horizontal(|ui| {
                            for col in 0..self.grid_cols {
                                // Calculate world position based on camera offset
                                let point = Point {
                                    x: col as i32 + camera_offset_x,
                                    y: row as i32 + camera_offset_y,
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
                                    println!(
                                        "Button clicked at world position: x: {}, y: {}",
                                        point.x, point.y
                                    );
                                }
                            }
                        });
                    }
                });
            });
        });
    }
}
/// Lists all available world files
pub fn get_world_files() -> Vec<PathBuf> {
    let worlds_dir = PathBuf::from("worlds");

    if !worlds_dir.exists() {
        return Vec::new();
    }

    let Ok(entries) = fs::read_dir(worlds_dir) else {
        return Vec::new();
    };

    entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("world"))
        .collect()
}
