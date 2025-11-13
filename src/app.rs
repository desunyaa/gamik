/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct TemplateApp {
    #[serde(skip)] // Recalculate on startup
    grid_size: usize,
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            grid_size: 1, // Will be recalculated
        }
    }
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.
        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        }
    }
}

impl eframe::App for TemplateApp {
    /// Called by the framework to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Get the font and calculate letter size
            let font_id = egui::TextStyle::Button.resolve(ui.style());
            let letter_galley = ui.fonts_mut(|f| {
                f.layout_no_wrap("A".to_string(), font_id.clone(), egui::Color32::WHITE)
            });

            // Get letter dimensions - use the larger dimension to make square buttons
            let letter_width = letter_galley.size().x;
            let letter_height = letter_galley.size().y;
            let letter_size = letter_width.max(letter_height);

            // Add padding around the letter for the button
            let padding = ui.spacing().button_padding;
            let button_size = letter_size + padding.x * 2.0;

            // Calculate available space
            let available_size = ui.available_size();

            // Calculate maximum number of buttons that can fit
            let max_cols = (available_size.x / button_size).floor() as usize;
            let max_rows = (available_size.y / button_size).floor() as usize;

            println!("max cols: {max_cols}");

            println!("max rows: {max_rows}");
            // Use the smaller dimension to maintain a square grid
            let grid_size = max_cols.min(max_rows).max(1); // At least 1x1
            self.grid_size = grid_size;

            // Center the grid
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    // Create the grid
                    for row in 0..self.grid_size {
                        ui.horizontal(|ui| {
                            for col in 0..self.grid_size {
                                let button = egui::Button::new("A")
                                    .min_size(egui::vec2(button_size, button_size));

                                if ui.add(button).clicked() {
                                    println!("Button clicked at row: {}, col: {}", row, col);
                                }
                            }
                        });
                    }
                });
            });

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                egui::warn_if_debug_build(ui);
            });
        });
    }
}
