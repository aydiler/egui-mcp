//! Example egui application with MCP bridge for E2E testing.

use egui_mcp_bridge::McpBridge;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn main() -> eframe::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Create the MCP bridge (starts TCP server on port 9876)
    let bridge = McpBridge::builder().port(9876).build();
    tracing::info!("MCP bridge listening on port {}", bridge.port());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([400.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "egui-mcp Test App",
        options,
        Box::new(|_cc| Ok(Box::new(TestApp::new(bridge)))),
    )
}

struct TestApp {
    bridge: McpBridge,
    // UI state
    name: String,
    brightness: f32,
    dark_mode: bool,
    selected_option: usize,
    click_count: u32,
    status_message: String,
    drag_text: String,
}

impl TestApp {
    fn new(bridge: McpBridge) -> Self {
        Self {
            bridge,
            name: String::new(),
            brightness: 50.0,
            dark_mode: false,
            selected_option: 0,
            click_count: 0,
            status_message: "Ready".into(),
            drag_text: "Drag across this text to test egui_drag. \
                        Selection should highlight as you drag."
                .into(),
        }
    }
}

impl eframe::App for TestApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        // Process commands and inject events BEFORE egui processes input
        self.bridge.process_commands();
        self.bridge.inject_raw_input(raw_input);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Enable AccessKit for MCP bridge
        ctx.enable_accesskit();
        // Clear widget registry for NEW frame
        self.bridge.begin_frame();
        // Request continuous repaints so we can process MCP commands
        ctx.request_repaint();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Test Application");
            ui.separator();

            // Text input
            ui.horizontal(|ui| {
                ui.label("Name:");
                let response = ui.text_edit_singleline(&mut self.name);
                self.bridge
                    .register_widget("Name", "text_input", &response, Some(&self.name));
            });

            ui.add_space(10.0);

            // Slider
            ui.horizontal(|ui| {
                ui.label("Brightness:");
                let response = ui.add(egui::Slider::new(&mut self.brightness, 0.0..=100.0));
                self.bridge.register_widget(
                    "Brightness",
                    "slider",
                    &response,
                    Some(&format!("{:.0}", self.brightness)),
                );
            });

            ui.add_space(10.0);

            // Checkbox
            let response = ui.checkbox(&mut self.dark_mode, "Dark Mode");
            self.bridge.register_widget(
                "Dark Mode",
                "checkbox",
                &response,
                Some(if self.dark_mode { "checked" } else { "unchecked" }),
            );

            ui.add_space(10.0);

            // Radio buttons
            ui.label("Select option:");
            ui.horizontal(|ui| {
                let r1 = ui.radio_value(&mut self.selected_option, 0, "Option A");
                self.bridge.register_widget(
                    "Option A",
                    "radio",
                    &r1,
                    Some(if self.selected_option == 0 {
                        "selected"
                    } else {
                        ""
                    }),
                );
                let r2 = ui.radio_value(&mut self.selected_option, 1, "Option B");
                self.bridge.register_widget(
                    "Option B",
                    "radio",
                    &r2,
                    Some(if self.selected_option == 1 {
                        "selected"
                    } else {
                        ""
                    }),
                );
                let r3 = ui.radio_value(&mut self.selected_option, 2, "Option C");
                self.bridge.register_widget(
                    "Option C",
                    "radio",
                    &r3,
                    Some(if self.selected_option == 2 {
                        "selected"
                    } else {
                        ""
                    }),
                );
            });

            ui.add_space(10.0);

            // Button with counter
            ui.horizontal(|ui| {
                let response = ui.button("Click Me");
                // Expose click_count as the widget's value so MCP tests can
                // verify response.clicked() actually fired via egui_get_value.
                self.bridge.register_widget(
                    "Click Me",
                    "button",
                    &response,
                    Some(&self.click_count.to_string()),
                );
                if response.clicked() {
                    self.click_count += 1;
                    self.status_message = format!("Button clicked {} times!", self.click_count);
                }
                ui.label(format!("Count: {}", self.click_count));
            });

            ui.add_space(10.0);

            // Reset button
            let reset_response = ui.button("Reset");
            self.bridge
                .register_widget("Reset", "button", &reset_response, None);
            if reset_response.clicked() {
                self.name.clear();
                self.brightness = 50.0;
                self.dark_mode = false;
                self.selected_option = 0;
                self.click_count = 0;
                self.status_message = "Reset!".into();
            }

            ui.add_space(10.0);

            // Drag target: multiline TextEdit. Drag the pointer across it via
            // `egui_drag` to verify the new drag tool produces a real selection.
            ui.label("Drag area (for egui_drag testing):");
            let drag_response = ui.add_sized(
                [380.0, 60.0],
                egui::TextEdit::multiline(&mut self.drag_text).desired_rows(3),
            );
            self.bridge.register_widget(
                "Drag Area",
                "text_area",
                &drag_response,
                Some(&self.drag_text),
            );

            ui.add_space(20.0);
            ui.separator();

            // Status
            ui.horizontal(|ui| {
                ui.label("Status:");
                ui.label(&self.status_message);
            });

            // Display current values
            ui.collapsing("Current Values", |ui| {
                ui.label(format!("Name: '{}'", self.name));
                ui.label(format!("Brightness: {:.1}", self.brightness));
                ui.label(format!("Dark Mode: {}", self.dark_mode));
                ui.label(format!(
                    "Selected: Option {}",
                    ["A", "B", "C"][self.selected_option]
                ));
                ui.label(format!("Click Count: {}", self.click_count));
            });
        });

        // Capture AccessKit output for MCP bridge
        self.bridge.capture_output(ctx);
    }
}
