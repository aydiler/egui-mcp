//! Managed widget responses for enforcing MCP registration.
//!
//! This module provides wrapper types that help ensure widgets are properly
//! registered for MCP testing. The key type is [`ManagedResponse`], which
//! wraps an egui [`Response`] and warns if dropped without registration.
//!
//! # Example
//!
//! ```rust,no_run
//! use egui_mcp_bridge::{McpBridge, McpResponseExt};
//!
//! fn ui_code(ui: &mut egui::Ui, bridge: &McpBridge) {
//!     // Wrap the response and register in one fluent chain
//!     if ui.button("Click Me")
//!         .managed()
//!         .register(bridge, "Click Button", "button", None)
//!         .clicked()
//!     {
//!         // Handle click
//!     }
//! }
//! ```

use crate::McpBridge;
use egui::Response;

/// A wrapped [`Response`] that tracks MCP registration status.
///
/// When dropped without being registered (via [`Self::register`] or
/// [`Self::skip_registration`]), this type will log a warning in debug
/// builds when the `mcp` feature is enabled.
///
/// This helps catch forgotten widget registrations during development.
pub struct ManagedResponse {
    // Option allows us to take() the response when consuming
    response: Option<Response>,
    registered: bool,
    widget_hint: Option<&'static str>,
}

impl ManagedResponse {
    /// Create a new managed response wrapping an egui Response.
    pub fn new(response: Response) -> Self {
        Self {
            response: Some(response),
            registered: false,
            widget_hint: None,
        }
    }

    /// Add a hint about what widget this is (for better warning messages).
    pub fn with_hint(mut self, hint: &'static str) -> Self {
        self.widget_hint = Some(hint);
        self
    }

    /// Register this widget with the MCP bridge and return the inner Response.
    ///
    /// This is the primary way to consume a ManagedResponse. It registers
    /// the widget for MCP testing and returns the original Response for
    /// further use (e.g., checking `.clicked()`).
    ///
    /// # Panics
    ///
    /// Panics if called more than once (the response has already been taken).
    pub fn register(
        mut self,
        bridge: &McpBridge,
        name: &str,
        widget_type: &str,
        value: Option<&str>,
    ) -> Response {
        let response = self.response.take().expect("Response already consumed");
        bridge.register_widget(name, widget_type, &response, value);
        self.registered = true;
        response
    }

    /// Register with just a name, inferring the widget type as "button".
    ///
    /// Convenience method for the common case of registering buttons.
    pub fn register_button(self, bridge: &McpBridge, name: &str) -> Response {
        self.register(bridge, name, "button", None)
    }

    /// Explicitly skip registration without triggering a warning.
    ///
    /// Use this when you intentionally don't want MCP coverage for a widget,
    /// such as decorative elements or widgets that are already covered by
    /// AccessKit's automatic tree.
    ///
    /// # Panics
    ///
    /// Panics if called more than once (the response has already been taken).
    pub fn skip_registration(mut self) -> Response {
        let response = self.response.take().expect("Response already consumed");
        self.registered = true; // Mark as handled
        response
    }

    /// Get a reference to the inner Response without consuming.
    ///
    /// Useful for checking properties before deciding whether to register.
    ///
    /// # Panics
    ///
    /// Panics if the response has already been consumed.
    pub fn response(&self) -> &Response {
        self.response.as_ref().expect("Response already consumed")
    }

    /// Check if clicked, consuming the ManagedResponse.
    ///
    /// **Warning**: This consumes without registration. Prefer using
    /// `.register().clicked()` for MCP-testable widgets.
    #[deprecated = "Use .register(bridge, name, type, value).clicked() for MCP testability"]
    pub fn clicked(mut self) -> bool {
        self.registered = true; // Suppress warning for intentional non-registration
        self.response.take().map(|r| r.clicked()).unwrap_or(false)
    }
}

impl Drop for ManagedResponse {
    fn drop(&mut self) {
        // Only warn if response wasn't consumed AND not registered
        if self.response.is_some() && !self.registered {
            let hint = self.widget_hint.unwrap_or("widget");
            // Only warn in debug builds to avoid production noise
            #[cfg(debug_assertions)]
            {
                tracing::warn!(
                    "ManagedResponse for '{}' dropped without MCP registration. \
                     Use .register() or .skip_registration() to silence this warning.",
                    hint
                );
            }
        }
    }
}

/// Extension trait to easily wrap egui Responses for MCP management.
///
/// This trait adds a `.managed()` method to [`Response`] that wraps it
/// in a [`ManagedResponse`] for registration tracking.
///
/// # Example
///
/// ```rust,no_run
/// use egui_mcp_bridge::McpResponseExt;
///
/// fn example(ui: &mut egui::Ui) {
///     let managed = ui.button("Test").managed();
///     // Now you must call .register() or .skip_registration()
/// }
/// ```
pub trait McpResponseExt {
    /// Wrap this Response in a [`ManagedResponse`] for registration tracking.
    fn managed(self) -> ManagedResponse;

    /// Wrap with a hint about the widget type (for better warnings).
    fn managed_as(self, hint: &'static str) -> ManagedResponse;
}

impl McpResponseExt for Response {
    fn managed(self) -> ManagedResponse {
        ManagedResponse::new(self)
    }

    fn managed_as(self, hint: &'static str) -> ManagedResponse {
        ManagedResponse::new(self).with_hint(hint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests would require egui test infrastructure
    // For now, they serve as documentation of expected behavior
}
