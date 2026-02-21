//! Application shell — wires game, UI, and networking together.

use crate::game::{self, Direction, EntityID, GameAction, GameState, Point};
use crate::net::{Message, ServerMessage, run_client_internal, run_server_internal};
use crate::ui;

use egui::{FontId, RichText};
use iroh::EndpointAddr;
use iroh::EndpointId;
use iroh::protocol::Router;
use std::fs;
use std::path::PathBuf;
use tokio::sync::mpsc;

// Toggle this constant to enable/disable test mode
const TEST_MODE: bool = true;

/// Which screen the application is currently showing.
#[derive(Debug, Clone, PartialEq)]
enum AppScreen {
    MainMenu,
    CharacterCreation,
    WorldCreation,
    CharacterSelection,
    WorldSelection,
    Playing,
}

pub struct GamikApp {
    player_id: EntityID,
    button_size: Option<f32>,
    menu_input_string: String,

    game: GameState,
    font_size: f32,
    router: Option<Router>,
    // Networking state
    server_to_client_rx: Option<mpsc::UnboundedReceiver<Message>>,
    client_to_server_tx: Option<mpsc::UnboundedSender<GameAction>>,
    screen: AppScreen,
    single_player: bool,

    // Test mode field
    test_mode_initialized: bool,
}

impl Default for GamikApp {
    fn default() -> Self {
        Self {
            menu_input_string: String::new(),
            router: None,
            screen: if TEST_MODE {
                AppScreen::Playing
            } else {
                AppScreen::MainMenu
            },
            player_id: EntityID(0),
            button_size: None,
            game: GameState::create_test_world("default".into()),
            font_size: 14.0,
            server_to_client_rx: None,
            client_to_server_tx: None,
            single_player: true,
            test_mode_initialized: false,
        }
    }
}

impl GamikApp {
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

        Self::default()
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

    fn start_server(&mut self, game: GameState) {
        let (router_tx, mut router_rx) = mpsc::unbounded_channel();

        // Spawn an async task to start the server
        tokio::spawn(async move {
            match run_server_internal(game).await {
                Ok(router) => {
                    // Send the router back to the main thread
                    let _ = router_tx.send(router);
                }
                Err(e) => eprintln!("Server error: {e}"),
            }
        });

        while self.router.is_none() {
            if let Ok(router) = router_rx.try_recv() {
                self.router = Some(router);
            }
        }
    }

    fn initialize_test_mode(&mut self) {
        if self.test_mode_initialized {
            return;
        }

        // Create or load a test world
        let test_world = get_world_files()
            .first()
            .and_then(|path| game::load_from_file(path).ok())
            .unwrap_or_else(|| {
                let world = GameState::create_test_world("test_world".into());
                let _ = game::save_to_file(&world);
                world
            });

        // Start server (blocking)
        self.start_server(test_world);

        // Connect as client
        if let Some(router) = &self.router {
            let eid = router.endpoint().addr();
            self.start_client(eid);
        }

        // Spawn test player
        if let Some(tx) = &self.client_to_server_tx {
            let _ = tx.send(GameAction::SpawnPlayer("TestPlayer".to_string()));
        }

        self.test_mode_initialized = true;
    }
}

impl eframe::App for GamikApp {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Initialize test mode once
        if TEST_MODE && !self.test_mode_initialized {
            self.initialize_test_mode();
        }

        // Poll network → update local game state copy
        self.poll_network();

        // Request continuous repainting to keep UI responsive
        ctx.request_repaint();

        match self.screen {
            AppScreen::MainMenu => {
                self.show_main_menu(ctx);
            }
            AppScreen::CharacterCreation => {
                self.show_character_creation_menu(ctx);
            }
            AppScreen::WorldCreation => {
                self.show_world_creation_menu(ctx);
            }
            AppScreen::CharacterSelection => {
                self.show_character_selection_menu(ctx);
            }
            AppScreen::WorldSelection => {
                self.show_world_selection_menu(ctx);
            }
            AppScreen::Playing => {
                // Collect input → game actions
                self.input(ctx);

                // Render
                self.rogue_screen(ctx);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Menu screens
// ---------------------------------------------------------------------------

impl GamikApp {
    /// Drain all pending network messages into local game state.
    fn poll_network(&mut self) {
        let Some(rx) = &mut self.server_to_client_rx else {
            return;
        };
        while let Ok(msg) = rx.try_recv() {
            if let Message::Server(smsg) = msg {
                match smsg {
                    ServerMessage::EntityMap(emap) => {
                        self.game.entities = emap;
                    }
                    ServerMessage::PlayerID(pid) => self.player_id = pid,
                }
            }
        }
    }

    fn show_main_menu(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);

                ui.heading(RichText::new("Roguelike Game").size(32.0));

                ui.add_space(50.0);

                if ui.button(RichText::new("Start Game").size(20.0)).clicked() {
                    self.single_player = true;
                    self.screen = AppScreen::WorldSelection;
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
                            self.screen = AppScreen::CharacterSelection;
                        }
                        Err(_) => {
                            self.screen = AppScreen::MainMenu;
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
                    self.screen = AppScreen::WorldCreation;
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
                                            if let Ok(world) = game::load_from_file(&world_path) {
                                                self.start_server(world);

                                                let eid =
                                                    self.router.as_ref().unwrap().endpoint().addr();
                                                self.start_client(eid);
                                                self.screen = AppScreen::CharacterSelection;
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
                    self.screen = AppScreen::MainMenu;
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
                    self.screen = AppScreen::CharacterCreation;
                }

                ui.add_space(30.0);

                let playables = self.game.get_playable_entities();

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
                                    .button(RichText::new(format!("{playable:#?}")).size(18.0))
                                    .clicked()
                                {
                                    if let Some(tx) = &self.client_to_server_tx {
                                        if let Err(e) = tx.send(GameAction::SpawnAs(playable)) {
                                            eprintln!("Failed to send game event: {e}");
                                        } else {
                                            self.screen = AppScreen::Playing;
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
                        self.screen = AppScreen::WorldSelection;
                    } else {
                        self.screen = AppScreen::MainMenu;
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

                ui.text_edit_singleline(&mut self.menu_input_string);

                ui.add_space(20.0);

                // Create Test World button
                if ui
                    .button(RichText::new("Create Test World").size(20.0))
                    .clicked()
                {
                    let world_name = if self.menu_input_string.trim().is_empty() {
                        "world_lol".to_string()
                    } else {
                        self.menu_input_string.trim().to_string()
                    };
                    self.menu_input_string.clear();
                    let new_world = GameState::create_test_world(world_name);
                    match game::save_to_file(&new_world) {
                        Ok(()) => {
                            self.screen = AppScreen::WorldSelection;
                        }
                        Err(e) => {
                            eprintln!("Failed to create world: {e}");
                        }
                    }
                }

                ui.add_space(20.0);

                // Back button
                if ui.button(RichText::new("Back").size(16.0)).clicked() {
                    self.screen = AppScreen::WorldSelection;
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

                // Text input for character name
                ui.label("Enter Character name:");
                ui.add_space(5.0);

                ui.text_edit_singleline(&mut self.menu_input_string);

                ui.add_space(20.0);

                // Create Character button
                if ui
                    .button(RichText::new("Create New Character").size(20.0))
                    .clicked()
                {
                    let char_name = if self.menu_input_string.trim().is_empty() {
                        "John".to_string()
                    } else {
                        self.menu_input_string.trim().to_string()
                    };
                    self.menu_input_string.clear();
                    if let Some(tx) = &self.client_to_server_tx {
                        if let Err(e) = tx.send(GameAction::SpawnPlayer(char_name)) {
                            eprintln!("Failed to send game event: {e}");
                        } else {
                            self.screen = AppScreen::CharacterSelection;
                        }
                    }
                }

                ui.add_space(20.0);

                // Back button
                if ui.button(RichText::new("Back").size(16.0)).clicked() {
                    self.screen = AppScreen::CharacterSelection;
                }
            });
        });
    }

    // -----------------------------------------------------------------------
    // Input
    // -----------------------------------------------------------------------

    pub fn input(&mut self, ctx: &egui::Context) {
        let mut messages_to_send = Vec::new();

        ctx.input(|i| {
            if i.key_pressed(egui::Key::W) || i.key_pressed(egui::Key::ArrowUp) {
                messages_to_send.push(GameAction::Move(Direction::Up));
            }

            if i.key_pressed(egui::Key::S) || i.key_pressed(egui::Key::ArrowDown) {
                messages_to_send.push(GameAction::Move(Direction::Down));
            }
            if i.key_pressed(egui::Key::A) || i.key_pressed(egui::Key::ArrowLeft) {
                messages_to_send.push(GameAction::Move(Direction::Left));
            }
            if i.key_pressed(egui::Key::D) || i.key_pressed(egui::Key::ArrowRight) {
                messages_to_send.push(GameAction::Move(Direction::Right));
            }
            if i.key_pressed(egui::Key::R) {
                messages_to_send.push(GameAction::SaveWorld);
            }
        });
        // Send all the collected messages
        if let Some(tx) = &self.client_to_server_tx {
            for event in messages_to_send {
                if let Err(e) = tx.send(event) {
                    eprintln!("Failed to send game event: {e}");
                }
            }
        }
    }

    fn rogue_screen(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("lol").show(ctx, |ui| {
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
                let char_galley = ui.fonts_mut(|f| {
                    f.layout_no_wrap(
                        chinese_char.to_string(),
                        font_id.clone(),
                        egui::Color32::WHITE,
                    )
                });

                let size = char_galley.size();
                self.button_size = Some(size.x.max(size.y));
            }

            let button_size = self.button_size.unwrap();

            // Calculate available space
            let content = ui.ctx().content_rect();
            let cols = ((content.width() / button_size) as usize).max(1);
            let rows = ((content.height() / button_size) as usize).max(1);

            // Camera centering
            let center = self
                .game
                .entities
                .get(&self.player_id)
                .map_or(Point { x: 0, y: 0 }, |e| e.position);

            let cam_x = center.x - (cols as i32 / 2);
            let cam_y = center.y - (rows as i32 / 2);

            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

            // Build spatial index once per frame for O(1) lookups
            let index = ui::build_spatial_index(&self.game.entities);

            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    for row in 0..rows {
                        ui.horizontal(|ui| {
                            for col in 0..cols {
                                let point = Point {
                                    x: col as i32 + cam_x,
                                    y: row as i32 + cam_y,
                                };

                                let glyph = ui::glyph_at(&index, &point);

                                let button = egui::Button::new(
                                    RichText::new(glyph.character)
                                        .color(glyph.fg_color)
                                        .font(FontId::proportional(
                                            self.font_size / glyph.size_mod,
                                        )),
                                )
                                .min_size(egui::vec2(button_size, button_size))
                                .corner_radius(0.0)
                                .fill(glyph.bg_color);
                                ui.add(button);
                            }
                        });
                    }
                });
            });
        });
    }
}

/// Lists all available world files.
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
