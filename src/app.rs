/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct TemplateApp {
    #[serde(skip)] // Recalculate on startup
    grid_cols: usize,
    #[serde(skip)]
    grid_rows: usize,
    #[serde(skip)]
    button_size: Option<f32>,
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            grid_cols: 1,      // Will be recalculated
            grid_rows: 1,      // Will be recalculated
            button_size: None, // Will be calculated on first frame
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
}

impl eframe::App for TemplateApp {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
            // Calculate button size on first frame if not already calculated
            if self.button_size.is_none() {
                let chinese_char = "中";
                let font_id = egui::TextStyle::Button.resolve(ui.style());
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

                self.button_size = Some(letter_size);
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
                                // Use @ sign for center button, Chinese character for others
                                let button_text = if row == center_row && col == center_col {
                                    "@"
                                } else {
                                    chinese_char
                                };

                                let button = egui::Button::new(button_text)
                                    .min_size(egui::vec2(button_size, button_size))
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

struct Entity(u32);
struct EntityGenerator(u32);

impl EntityGenerator {
    fn default() -> Self {
        EntityGenerator(0)
    }

    fn new_entity(&mut self) -> Entity {
        self.0 += 1;

        Entity(self.0.clone())
    }
}
struct Point {
    x: u32,
    y: u32,
}
struct PointEntityMap(rustc_hash::FxHashMap<Point, Vec<Entity>>);

struct EntityPointMap(rustc_hash::FxHashMap<Entity, Point>);
struct GameWorld {
    spatial: SpatialWorld,
    player: Entity,
}

struct SpatialWorld {
    pemap: PointEntityMap,
    epmap: EntityPointMap,
}

impl SpatialWorld {
    fn insert_ent_at_point(&mut self, ent: Entity, point: Point) {}
}
