//! Extension trait for egui::Ui that provides MCP-aware widget methods.
//!
//! This module provides [`McpUiExt`], an extension trait that adds MCP-registered
//! versions of common egui widgets. These methods automatically register widgets
//! for MCP testing without requiring explicit `#[cfg(feature = "mcp")]` blocks.
//!
//! # Setup
//!
//! At the start of each frame, store the bridge in the egui context:
//!
//! ```rust,no_run
//! use egui_mcp_bridge::McpBridge;
//!
//! fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
//!     // Store bridge in context for McpUiExt to access
//!     self.bridge.store_in_context(ctx);
//!
//!     // Now use McpUiExt methods in any UI code
//!     egui::CentralPanel::default().show(ctx, |ui| {
//!         use egui_mcp_bridge::McpUiExt;
//!         if ui.mcp_button("Save", "Save File").clicked() {
//!             // ...
//!         }
//!     });
//! }
//! ```
//!
//! # Example
//!
//! ```rust,no_run
//! use egui_mcp_bridge::McpUiExt;
//!
//! fn render_toolbar(ui: &mut egui::Ui) {
//!     // Instead of:
//!     // if ui.button("Save").clicked() { ... }
//!     // #[cfg(feature = "mcp")]
//!     // bridge.register_widget("Save", "button", &response, None);
//!
//!     // Just write:
//!     if ui.mcp_button("Save", "Save").clicked() {
//!         // Handle save
//!     }
//!
//!     // For icon-only buttons, the name provides semantic meaning:
//!     if ui.mcp_small_button("Refresh", "↻").clicked() {
//!         // Handle refresh
//!     }
//! }
//! ```

use crate::McpBridge;
use egui::{Response, Ui, WidgetText};

/// ID used to store McpBridge in egui's context data.
const MCP_CONTEXT_ID: &str = "egui_mcp_bridge";

impl McpBridge {
    /// Store this bridge in the egui context for use by [`McpUiExt`] methods.
    ///
    /// Call this once at the beginning of each frame, before any UI code.
    /// This enables the `McpUiExt` extension methods to automatically register
    /// widgets without needing explicit bridge references.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    ///     self.bridge.store_in_context(ctx);
    ///     // ... rest of update
    /// }
    /// ```
    pub fn store_in_context(&self, ctx: &egui::Context) {
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new(MCP_CONTEXT_ID), self.clone());
        });
    }

    /// Retrieve the bridge from the egui context, if stored.
    ///
    /// This is used internally by [`McpUiExt`] methods.
    pub fn from_context(ctx: &egui::Context) -> Option<Self> {
        ctx.data(|d| d.get_temp::<Self>(egui::Id::new(MCP_CONTEXT_ID)))
    }
}

/// Extension trait for [`egui::Ui`] that provides MCP-registered widget methods.
///
/// These methods create standard egui widgets and automatically register them
/// with the MCP bridge for E2E testing. The bridge must first be stored in
/// the egui context using [`McpBridge::store_in_context`].
///
/// If no bridge is stored in the context, these methods still work normally
/// but skip the MCP registration (graceful degradation).
pub trait McpUiExt {
    /// Create a button and register it with the MCP bridge.
    ///
    /// # Arguments
    /// * `name` - Semantic name for MCP testing (e.g., "Save", "Submit")
    /// * `text` - Display text for the button
    ///
    /// # Example
    /// ```rust,no_run
    /// use egui_mcp_bridge::McpUiExt;
    ///
    /// fn ui(ui: &mut egui::Ui) {
    ///     if ui.mcp_button("Save Document", "Save").clicked() {
    ///         // Handle click
    ///     }
    ///     // Dynamic names work too:
    ///     if ui.mcp_button(format!("Tab: {}", filename), &filename).clicked() {
    ///         // Handle click
    ///     }
    /// }
    /// ```
    fn mcp_button(&mut self, name: impl AsRef<str>, text: impl Into<WidgetText>) -> Response;

    /// Create a small button and register it with the MCP bridge.
    ///
    /// Particularly useful for icon-only buttons where the icon alone
    /// doesn't provide semantic meaning for testing.
    ///
    /// # Arguments
    /// * `name` - Semantic name for MCP testing (e.g., "Close Tab", "Refresh")
    /// * `text` - Display text/icon for the button
    ///
    /// # Example
    /// ```rust,no_run
    /// use egui_mcp_bridge::McpUiExt;
    ///
    /// fn ui(ui: &mut egui::Ui) {
    ///     if ui.mcp_small_button("Close Tab", "×").clicked() {
    ///         // Handle close
    ///     }
    /// }
    /// ```
    fn mcp_small_button(&mut self, name: impl AsRef<str>, text: impl Into<WidgetText>) -> Response;

    /// Create a selectable label and register it with the MCP bridge.
    ///
    /// # Arguments
    /// * `name` - Semantic name for MCP testing
    /// * `selected` - Whether the label is currently selected
    /// * `text` - Display text for the label
    ///
    /// # Example
    /// ```rust,no_run
    /// use egui_mcp_bridge::McpUiExt;
    ///
    /// fn ui(ui: &mut egui::Ui, is_active: bool) {
    ///     if ui.mcp_selectable_label("Tab: README", is_active, "README.md").clicked() {
    ///         // Handle selection
    ///     }
    /// }
    /// ```
    fn mcp_selectable_label(
        &mut self,
        name: impl AsRef<str>,
        selected: bool,
        text: impl Into<WidgetText>,
    ) -> Response;

    /// Create a selectable label with a custom value and register it with the MCP bridge.
    ///
    /// # Arguments
    /// * `name` - Semantic name for MCP testing
    /// * `selected` - Whether the label is currently selected
    /// * `text` - Display text for the label
    /// * `value` - Custom value to report in MCP snapshots
    fn mcp_selectable_label_with_value(
        &mut self,
        name: impl AsRef<str>,
        selected: bool,
        text: impl Into<WidgetText>,
        value: impl AsRef<str>,
    ) -> Response;

    /// Create a checkbox and register it with the MCP bridge.
    ///
    /// # Arguments
    /// * `name` - Semantic name for MCP testing
    /// * `checked` - Mutable reference to the checkbox state
    /// * `text` - Display text for the checkbox
    ///
    /// # Example
    /// ```rust,no_run
    /// use egui_mcp_bridge::McpUiExt;
    ///
    /// fn ui(ui: &mut egui::Ui, dark_mode: &mut bool) {
    ///     ui.mcp_checkbox("Dark Mode", dark_mode, "Enable dark mode");
    /// }
    /// ```
    fn mcp_checkbox(
        &mut self,
        name: impl AsRef<str>,
        checked: &mut bool,
        text: impl Into<WidgetText>,
    ) -> Response;

    /// Create a label and register it with the MCP bridge.
    ///
    /// Use this for labels that display testable state.
    ///
    /// # Arguments
    /// * `name` - Semantic name for MCP testing
    /// * `text` - Display text for the label
    /// * `value` - Optional value to report in MCP snapshots
    fn mcp_label(
        &mut self,
        name: impl AsRef<str>,
        text: impl Into<WidgetText>,
        value: Option<&str>,
    ) -> Response;

    /// Create a hyperlink and register it with the MCP bridge.
    ///
    /// # Arguments
    /// * `name` - Semantic name for MCP testing
    /// * `url` - URL to link to
    /// * `text` - Display text for the link
    fn mcp_hyperlink(&mut self, name: impl AsRef<str>, url: &str, text: impl Into<WidgetText>) -> Response;
}

impl McpUiExt for Ui {
    fn mcp_button(&mut self, name: impl AsRef<str>, text: impl Into<WidgetText>) -> Response {
        let response = self.button(text);
        register_response(self, name.as_ref(), "button", &response, None);
        response
    }

    fn mcp_small_button(&mut self, name: impl AsRef<str>, text: impl Into<WidgetText>) -> Response {
        let response = self.small_button(text);
        register_response(self, name.as_ref(), "button", &response, None);
        response
    }

    fn mcp_selectable_label(
        &mut self,
        name: impl AsRef<str>,
        selected: bool,
        text: impl Into<WidgetText>,
    ) -> Response {
        let response = self.selectable_label(selected, text);
        let value = if selected { "selected" } else { "" };
        register_response(self, name.as_ref(), "selectable", &response, Some(value));
        response
    }

    fn mcp_selectable_label_with_value(
        &mut self,
        name: impl AsRef<str>,
        selected: bool,
        text: impl Into<WidgetText>,
        value: impl AsRef<str>,
    ) -> Response {
        let response = self.selectable_label(selected, text);
        register_response(self, name.as_ref(), "selectable", &response, Some(value.as_ref()));
        response
    }

    fn mcp_checkbox(
        &mut self,
        name: impl AsRef<str>,
        checked: &mut bool,
        text: impl Into<WidgetText>,
    ) -> Response {
        let response = self.checkbox(checked, text);
        let value = if *checked { "checked" } else { "unchecked" };
        register_response(self, name.as_ref(), "checkbox", &response, Some(value));
        response
    }

    fn mcp_label(
        &mut self,
        name: impl AsRef<str>,
        text: impl Into<WidgetText>,
        value: Option<&str>,
    ) -> Response {
        let response = self.label(text);
        register_response(self, name.as_ref(), "label", &response, value);
        response
    }

    fn mcp_hyperlink(&mut self, name: impl AsRef<str>, url: &str, text: impl Into<WidgetText>) -> Response {
        let response = self.hyperlink_to(text, url);
        register_response(self, name.as_ref(), "link", &response, Some(url));
        response
    }
}

/// Internal helper to register a response with the MCP bridge from context.
fn register_response(ui: &Ui, name: &str, widget_type: &str, response: &Response, value: Option<&str>) {
    if let Some(bridge) = McpBridge::from_context(ui.ctx()) {
        bridge.register_widget(name, widget_type, response, value);
    }
}

#[cfg(test)]
mod tests {
    // Tests would require egui test infrastructure
}
