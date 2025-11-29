use crate::structs::*;

use crate::network::*;
use egui::{FontId, RichText};
use iroh::EndpointAddr;
use iroh::protocol::Router;
use tokio::sync::mpsc;
pub struct TemplateApp {
    player_id: EntityID,
    grid_cols: usize,
    grid_rows: usize,
    button_size: Option<f32>,

    multiplayer_endpoint_input: String,
    world: GameWorld,
    font_size: f32,
    router: Option<Router>,
    // Networking state
    server_to_client_rx: Option<mpsc::UnboundedReceiver<Message>>,
    client_to_server_tx: Option<mpsc::UnboundedSender<GameEvent>>, // New field
    game_state: GameState,
}
#[derive(Debug, Clone, PartialEq)]
enum GameState {
    MainMenu,
    Connecting,
    Playing,
}
impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            multiplayer_endpoint_input: String::new(),
            router: None,
            game_state: GameState::MainMenu,
            player_id: EntityID(0),
            grid_cols: 1,
            grid_rows: 1,
            button_size: None,
            world: GameWorld::create_test_world(),
            font_size: 20.0,
            server_to_client_rx: None,
            client_to_server_tx: None, // Initialize as None
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

        let mut app = Self::default();

        // Start the networking
        app.start_server();
        let eid = app.router.as_ref().unwrap().endpoint().id();
        app.start_client(eid);

        app
    }

    fn start_client(&mut self, addr: impl Into<EndpointAddr>) {
        let (msg_tx, msg_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        self.server_to_client_rx = Some(msg_rx);
        self.client_to_server_tx = Some(event_tx);
        let server_addr = self.router.as_ref().unwrap().endpoint().id();
        // Spawn the networking tasks
        tokio::spawn(async move {
            run_client_internal(server_addr, msg_tx, event_rx).await;
        });
    }
    fn start_server(&mut self) {
        let (router_tx, mut router_rx) = mpsc::unbounded_channel();

        // Spawn an async task to start the server
        tokio::spawn(async move {
            match run_server_internal().await {
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
            while let Ok(smsg) = rx.try_recv() {
                if let Message::ServerMessage(ServerMessage::EntityMap(emap)) = smsg {
                    for (eid, e) in emap.iter() {
                        self.world.entities.insert(eid.clone(), e.clone());
                    }
                }
            }
        }

        // Request continuous repainting to keep UI responsive
        ctx.request_repaint();
        match self.game_state {
            GameState::MainMenu => {
                self.show_main_menu(ctx);
            }
            GameState::Connecting => {
                self.show_connecting_screen(ctx);
            }
            GameState::Playing => {
                // Check for new message counts from server
                if let Some(rx) = &mut self.server_to_client_rx {
                    while let Ok(smsg) = rx.try_recv() {
                        if let Message::ServerMessage(ServerMessage::EntityMap(emap)) = smsg {
                            for (eid, e) in emap.iter() {
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
                ui.add_space(100.0);

                ui.heading(RichText::new("Roguelike Game").size(32.0));
                ui.add_space(50.0);

                if ui
                    .button(RichText::new("Start Singleplayer Game").size(20.0))
                    .clicked()
                {
                    println!("LOL");
                }

                ui.add_space(20.0);

                if ui
                    .button(RichText::new("Host Multiplayer Game").size(20.0))
                    .clicked()
                {
                    println!("LOL");
                }

                ui.add_space(20.0);

                ui.separator();
                ui.add_space(20.0);

                ui.heading("Join Multiplayer Game");
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    ui.label("Endpoint ID:");
                    ui.text_edit_singleline(&mut self.multiplayer_endpoint_input);
                });

                ui.add_space(10.0);

                if ui.button(RichText::new("Connect").size(18.0)).clicked() {
                    println!("LOL");
                }
            });
        });
    }

    fn show_connecting_screen(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(200.0);

                ui.heading("lol");
                ui.add_space(20.0);
                ui.spinner();

                ui.add_space(40.0);

                if ui.button("Cancel").clicked() {
                    self.game_state = GameState::MainMenu;
                }
            });
        });
    }
    pub fn input(&mut self, ctx: &egui::Context) {
        let mut messages_to_send = Vec::new();

        ctx.input(|i| {
            if i.key_pressed(egui::Key::W) || i.key_pressed(egui::Key::ArrowUp) {
                messages_to_send.push(GameEvent::Move {
                    entity: self.world.player,
                    direction: Direction::Up,
                });
            }

            if i.key_pressed(egui::Key::Q) {
                panic!();
            }

            if i.key_pressed(egui::Key::S) || i.key_pressed(egui::Key::ArrowDown) {
                messages_to_send.push(GameEvent::Move {
                    entity: self.world.player,
                    direction: Direction::Down,
                });
            }
            if i.key_pressed(egui::Key::A) || i.key_pressed(egui::Key::ArrowLeft) {
                messages_to_send.push(GameEvent::Move {
                    entity: self.world.player,
                    direction: Direction::Left,
                });
            }
            if i.key_pressed(egui::Key::D) || i.key_pressed(egui::Key::ArrowRight) {
                messages_to_send.push(GameEvent::Move {
                    entity: self.world.player,
                    direction: Direction::Right,
                });
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

            // Calculate available space
            let available_size = ui.available_size();

            // Calculate maximum number of buttons that can fit
            let max_cols = (available_size.x / button_size).floor() as usize;
            let max_rows = (available_size.y / button_size).floor() as usize;

            // Use all available space
            self.grid_cols = max_cols.max(1);
            self.grid_rows = max_rows.max(1);

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
                                    x: col as i32,
                                    y: row as i32,
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
