use crate::structs::*;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bincode::{Decode, Encode};
use iroh::{
    Endpoint, EndpointAddr,
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler, Router},
};
use n0_error::{Result, StdResultExt};

use egui::{FontId, RichText};
use tokio::sync::mpsc;

const ALPN: &[u8] = b"iroh-example/echo/0";
const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024; // 10MB limit

#[derive(Debug, Clone, Encode, Decode)]
enum ClientMessage {
    GameMessage(GameEvent),
}

#[derive(Debug, Clone, Encode, Decode)]
enum ServerMessage {
    EntityMap(EntityMap),
}

#[derive(Debug, Clone, Encode, Decode)]
enum Message {
    ClientMessage(ClientMessage),
    ServerMessage(ServerMessage),
    Blank,
}

// ====================
// Unidirectional Stream Solution
// ====================

/// Send one message on a new unidirectional stream
async fn send_one_way(conn: &Connection, msg: &Message) -> Result<()> {
    let mut send = conn.open_uni().await.anyerr()?;

    let encoded = bincode::encode_to_vec(msg, bincode::config::standard())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    send.write_all(&encoded).await.anyerr()?;
    send.finish().anyerr()?;

    Ok(())
}

/// Receive one message from a unidirectional stream
async fn recv_one_way(mut recv: iroh::endpoint::RecvStream) -> Result<Message> {
    let bytes = recv.read_to_end(MAX_MESSAGE_SIZE).await.anyerr()?;

    let (msg, _) = bincode::decode_from_slice(&bytes, bincode::config::standard())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    Ok(msg)
}

async fn run_server_internal() -> Result<Router> {
    let endpoint = Endpoint::bind().await?;
    let router = Router::builder(endpoint).accept(ALPN, Echo::new()).spawn();
    println!("Server started at {:#?}", router.endpoint().addr());
    Ok(router)
}

async fn run_client_internal(addr: EndpointAddr, tx: mpsc::UnboundedSender<Message>) -> Result<()> {
    let endpoint = Endpoint::bind().await?;
    let conn = endpoint.connect(addr, ALPN).await?;

    // Spawn a task to receive messages from server
    let conn_clone = conn.clone();
    tokio::spawn(async move {
        loop {
            match conn_clone.accept_uni().await {
                Ok(recv) => {
                    match recv_one_way(recv).await {
                        Ok(msg) => {
                            // Send the server's count to the UI
                            let _ = tx.send(msg.clone());
                            println!("Client received server count: {:#?}", msg);
                        }
                        Ok(Message::ClientMessage(_)) => {
                            eprintln!("Client received unexpected ClientMessage");
                        }
                        Err(e) => {
                            eprintln!("Error receiving server message: {}", e);
                        }
                    }
                }
                Err(_) => {
                    println!("Server connection closed");
                    break;
                }
            }
        }
    });

    // Send messages to server
    let mut message_count = 0u64;
    let client_msg = ClientMessage::GameMessage(GameEvent::Move {
        entity: EntityID(5),
        direction: Direction::Up,
    });

    loop {
        let msg = Message::ClientMessage(client_msg.clone());

        match send_one_way(&conn, &msg).await {
            Ok(_) => {
                message_count += 1;

                if message_count % 10 == 0 {
                    println!("Client sent {} messages", message_count);
                }
            }
            Err(e) => {
                eprintln!("Error sending message: {}", e);
                break;
            }
        }

        // Uncomment to slow down message sending
        // tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct Echo {
    receive_count: Arc<AtomicU64>,
}

impl Echo {
    fn new() -> Self {
        Self {
            receive_count: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl ProtocolHandler for Echo {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let endpoint_id = connection.remote_id();
        println!("Accepted connection from {}", endpoint_id);

        // Accept unidirectional streams in a loop
        loop {
            match connection.accept_uni().await {
                Ok(recv) => {
                    // Increment the shared counter atomically
                    let current_count = self.receive_count.fetch_add(1, Ordering::SeqCst);
                    let new_count = current_count + 1;

                    let conn_clone = connection.clone();

                    // Spawn a task to handle each stream independently
                    tokio::spawn(async move {
                        match recv_one_way(recv).await {
                            Ok(Message::ClientMessage(msg)) => {
                                // Just log occasionally to avoid spam
                                if current_count % 10 == 0 {
                                    println!(
                                        "Server received message #{}: {:?}",
                                        current_count, msg
                                    );
                                }

                                // Send the count back to the client
                                let response = Message::ServerMessage(ServerMessage::EntityMap(
                                    EntityMap::default(),
                                ));
                                if let Err(e) = send_one_way(&conn_clone, &response).await {
                                    eprintln!("Error sending count to client: {}", e);
                                }
                            }
                            Ok(Message::ServerMessage(_)) => {
                                eprintln!("Server received unexpected ServerMessage");
                            }
                            Ok(Message::Blank) => {
                                eprintln!("Server received unexpected ServerMessage");
                            }
                            Err(e) => {
                                eprintln!("Error receiving message: {}", e);
                            }
                        }
                    });
                }
                Err(_) => {
                    // Connection closed
                    let total_count = self.receive_count.load(Ordering::SeqCst);
                    println!(
                        "Connection closed. Total messages received: {}",
                        total_count
                    );
                    break;
                }
            }
        }

        Ok(())
    }
}

pub struct TemplateApp {
    player_id: EntityID,
    grid_cols: usize,
    grid_rows: usize,
    button_size: Option<f32>,

    world: GameWorld,
    net_world: GameWorld,
    font_size: f32,

    // Networking state
    message_count: Message,
    message_rx: Option<mpsc::UnboundedReceiver<Message>>,
    _router: Option<Router>,
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            player_id: EntityID(0),
            grid_cols: 1,
            grid_rows: 1,
            button_size: None,
            world: GameWorld::create_test_world(),
            net_world: GameWorld::create_test_world(),
            font_size: 20.0,
            message_count: Message::Blank,
            message_rx: None,
            _router: None,
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
        app.start_singleplayer();

        app
    }

    fn start_singleplayer(&mut self) {
        let (tx, rx) = mpsc::unbounded_channel();
        self.message_rx = Some(rx);

        // Spawn the networking tasks
        tokio::spawn(async move {
            match run_singleplayer_internal(tx).await {
                Ok(_) => println!("Singleplayer mode finished"),
                Err(e) => eprintln!("Singleplayer error: {}", e),
            }
        });
    }
}

async fn run_singleplayer_internal(tx: mpsc::UnboundedSender<Message>) -> Result<()> {
    let router = run_server_internal().await?;
    router.endpoint().online().await;
    let server_addr = router.endpoint().addr();

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Run client (will run infinitely)
    run_client_internal(server_addr, tx).await?;

    Ok(())
}

impl eframe::App for TemplateApp {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for new message counts from server
        if let Some(rx) = &mut self.message_rx {
            while let Ok(count) = rx.try_recv() {
                self.message_count = count;
            }
        }

        // Request continuous repainting to keep UI responsive
        ctx.request_repaint();

        // Handle keyboard input
        self.input(ctx);

        // Process all events
        self.net_world.process_events();

        let cl_info = self.net_world.gen_client_info(self.player_id);

        for (eid, e) in cl_info.iter() {
            self.world.entities.insert(eid.clone(), e.clone());
        }

        // Right sidebar
        self.right_panel(ctx);

        // Bottom bar
        self.bottom_panel(ctx);

        // Central panel with letter grid
        self.rogue_screen(ctx);
    }
}

impl TemplateApp {
    pub fn input(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            if i.key_pressed(egui::Key::W) || i.key_pressed(egui::Key::ArrowUp) {
                self.net_world.event_queue.push(GameEvent::Move {
                    entity: self.world.player,
                    direction: Direction::Up,
                });
            }

            if i.key_pressed(egui::Key::Q) {
                panic!();
            }

            if i.key_pressed(egui::Key::S) || i.key_pressed(egui::Key::ArrowDown) {
                self.net_world.event_queue.push(GameEvent::Move {
                    entity: self.world.player,
                    direction: Direction::Down,
                });
            }
            if i.key_pressed(egui::Key::A) || i.key_pressed(egui::Key::ArrowLeft) {
                self.net_world.event_queue.push(GameEvent::Move {
                    entity: self.world.player,
                    direction: Direction::Left,
                });
            }
            if i.key_pressed(egui::Key::D) || i.key_pressed(egui::Key::ArrowRight) {
                self.net_world.event_queue.push(GameEvent::Move {
                    entity: self.world.player,
                    direction: Direction::Right,
                });
            }
        });
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
            .show(ctx, |ui| {
                ui.heading("Bottom Bar");
                ui.separator();
                ui.label(format!(
                    "Messages received by server: {:#?}",
                    self.message_count
                ));
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
