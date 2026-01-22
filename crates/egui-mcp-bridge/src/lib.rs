//! egui-mcp-bridge: Bridge library enabling MCP-based E2E testing for egui applications.
//!
//! # Example
//!
//! ```rust,no_run
//! use egui_mcp_bridge::McpBridge;
//!
//! fn main() {
//!     let bridge = McpBridge::builder().port(9876).build();
//!     // Pass bridge to your eframe app and call bridge methods in update()
//! }
//! ```
//!
//! # Ensuring Widget Registration
//!
//! For widgets that need explicit registration (like icon-only buttons),
//! use the [`ManagedResponse`] wrapper to catch forgotten registrations:
//!
//! ```rust,no_run
//! use egui_mcp_bridge::{McpBridge, McpResponseExt};
//!
//! fn ui_code(ui: &mut egui::Ui, bridge: &McpBridge) {
//!     // This will warn if you forget to register
//!     if ui.button("⊞")
//!         .managed_as("expand button")
//!         .register_button(bridge, "Expand All")
//!         .clicked()
//!     {
//!         // Handle click
//!     }
//! }
//! ```

pub mod events;
pub mod managed;
pub mod protocol;
pub mod server;
pub mod tree;
pub mod ui_ext;

// Re-export managed response types for convenience
pub use managed::{ManagedResponse, McpResponseExt};
// Re-export UI extension trait
pub use ui_ext::McpUiExt;

use egui::accesskit::{ActionRequest, TreeUpdate};
use egui::{Context, Rect, Response};
use events::EventQueue;
use protocol::{SnapshotResponse, ValueResponse};
use server::{BridgeCommand, BridgeServer};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tree::SerializedTree;

/// Simplified widget info for MCP bridge.
#[derive(Debug, Clone)]
pub struct WidgetInfo {
    pub id: u64,
    pub name: String,
    pub widget_type: String,
    pub rect: Rect,
    pub value: Option<String>,
    pub enabled: bool,
}

/// Coverage statistics for MCP widget registration.
///
/// Returned by [`McpBridge::get_coverage`] to help diagnose
/// whether widgets are properly registered for testing.
#[derive(Debug, Clone, Copy)]
pub struct FrameCoverage {
    /// Number of nodes in the AccessKit tree (automatic coverage).
    pub accesskit_nodes: usize,
    /// Number of manually registered widgets.
    pub registered_widgets: usize,
}

impl FrameCoverage {
    /// Total interactive elements available for MCP testing.
    pub fn total(&self) -> usize {
        // Note: These can overlap, so this is an upper bound
        self.accesskit_nodes + self.registered_widgets
    }

    /// Check if coverage seems reasonable.
    ///
    /// Returns `true` if there's at least some coverage from either
    /// AccessKit or manual registration.
    pub fn is_reasonable(&self) -> bool {
        self.accesskit_nodes > 0 || self.registered_widgets > 0
    }
}

/// Pending drag operation that spans multiple frames.
#[derive(Debug, Clone)]
struct PendingDrag {
    start_pos: egui::Pos2,
    end_pos: egui::Pos2,
    phase: DragPhase,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DragPhase {
    MoveToStart,
    Press,
    Drag,
    Release,
    Done,
}

/// Pending click operation that spans multiple frames.
#[derive(Debug, Clone)]
struct PendingClick {
    pos: egui::Pos2,
    phase: ClickPhase,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ClickPhase {
    Move,
    Press,
    Release,
    Done,
}

/// Configuration for the MCP bridge.
#[derive(Debug, Clone)]
pub struct McpBridgeConfig {
    pub port: u16,
}

impl Default for McpBridgeConfig {
    fn default() -> Self {
        Self { port: 9876 }
    }
}

/// Builder for McpBridge.
pub struct McpBridgeBuilder {
    config: McpBridgeConfig,
}

impl McpBridgeBuilder {
    pub fn new() -> Self {
        Self {
            config: McpBridgeConfig::default(),
        }
    }

    pub fn port(mut self, port: u16) -> Self {
        self.config.port = port;
        self
    }

    pub fn build(self) -> McpBridge {
        McpBridge::new(self.config)
    }
}

impl Default for McpBridgeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Main bridge type that egui apps hold.
///
/// This type is `Clone` - cloning creates a new handle to the same underlying
/// bridge instance. This is useful for storing in egui's context data.
#[derive(Clone)]
pub struct McpBridge {
    config: McpBridgeConfig,
    inner: Arc<Mutex<BridgeInner>>,
    command_rx: Arc<Mutex<mpsc::Receiver<BridgeCommand>>>,
    pending_actions: Arc<Mutex<Vec<ActionRequest>>>,
    runtime_handle: tokio::runtime::Handle,
    // Note: Runtime is not cloned - only the original instance owns it
    _runtime: Arc<Option<tokio::runtime::Runtime>>,
}

struct BridgeInner {
    tree: SerializedTree,
    event_queue: EventQueue,
    widgets: HashMap<u64, WidgetInfo>,
    widget_counter: u64,
    pending_drag: Option<PendingDrag>,
    pending_click: Option<PendingClick>,
}

impl McpBridge {
    /// Create a builder for configuring the bridge.
    pub fn builder() -> McpBridgeBuilder {
        McpBridgeBuilder::new()
    }

    /// Create a new bridge with the given config.
    fn new(config: McpBridgeConfig) -> Self {
        let (command_tx, command_rx) = mpsc::channel(32);

        // Create tokio runtime for the server
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");

        // Get handle before moving runtime
        let runtime_handle = runtime.handle().clone();

        // Start the TCP server
        let port = config.port;
        let server = BridgeServer::new(command_tx);
        runtime.spawn(async move {
            if let Err(e) = server.run(port).await {
                tracing::error!("Bridge server error: {}", e);
            }
        });

        Self {
            config,
            inner: Arc::new(Mutex::new(BridgeInner {
                tree: SerializedTree::new(),
                event_queue: EventQueue::new(),
                widgets: HashMap::new(),
                widget_counter: 0,
                pending_drag: None,
                pending_click: None,
            })),
            command_rx: Arc::new(Mutex::new(command_rx)),
            pending_actions: Arc::new(Mutex::new(Vec::new())),
            runtime_handle,
            _runtime: Arc::new(Some(runtime)),
        }
    }

    /// Get the port the bridge is listening on.
    pub fn port(&self) -> u16 {
        self.config.port
    }

    /// Clear widget registry for a new frame.
    /// Call this AFTER process_commands() but BEFORE registering widgets.
    pub fn begin_frame(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.widgets.clear();
        inner.widget_counter = 0;
    }

    /// Register a widget for MCP access.
    /// Returns a unique ID that can be used as a ref.
    pub fn register_widget(
        &self,
        name: &str,
        widget_type: &str,
        response: &Response,
        value: Option<&str>,
    ) -> u64 {
        let mut inner = self.inner.lock().unwrap();
        inner.widget_counter += 1;
        let id = inner.widget_counter;
        inner.widgets.insert(
            id,
            WidgetInfo {
                id,
                name: name.to_string(),
                widget_type: widget_type.to_string(),
                rect: response.rect,
                value: value.map(|s| s.to_string()),
                enabled: response.sense.senses_click() || response.sense.senses_drag(),
            },
        );
        id
    }

    /// Register a widget using a rect directly (for widgets in closures where Response isn't available).
    /// Assumes the widget is enabled (clickable).
    /// Returns a unique ID that can be used as a ref.
    pub fn register_widget_rect(
        &self,
        name: &str,
        widget_type: &str,
        rect: Rect,
        value: Option<&str>,
    ) -> u64 {
        let mut inner = self.inner.lock().unwrap();
        inner.widget_counter += 1;
        let id = inner.widget_counter;
        inner.widgets.insert(
            id,
            WidgetInfo {
                id,
                name: name.to_string(),
                widget_type: widget_type.to_string(),
                rect,
                value: value.map(|s| s.to_string()),
                enabled: true, // Assume clickable
            },
        );
        id
    }

    /// Get the widget registry formatted as a tree string.
    pub fn format_widget_tree(&self) -> String {
        let inner = self.inner.lock().unwrap();
        if inner.widgets.is_empty() {
            return "(no widgets registered)".to_string();
        }

        let mut output = String::new();
        let mut widgets: Vec<_> = inner.widgets.values().collect();
        widgets.sort_by_key(|w| w.id);

        for widget in widgets {
            output.push_str(&format!(
                "- {} \"{}\" [ref=n{}]",
                widget.widget_type, widget.name, widget.id
            ));
            if let Some(ref value) = widget.value {
                output.push_str(&format!(": \"{}\"", value));
            }
            if !widget.enabled {
                output.push_str(" [disabled]");
            }
            output.push('\n');
        }
        output
    }

    /// Get widget count.
    pub fn widget_count(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.widgets.len()
    }

    /// Get coverage statistics for the current frame.
    ///
    /// Returns information about how many widgets are registered vs
    /// how many AccessKit nodes exist. Useful for debugging MCP coverage.
    pub fn get_coverage(&self) -> FrameCoverage {
        let inner = self.inner.lock().unwrap();
        FrameCoverage {
            accesskit_nodes: inner.tree.node_count(),
            registered_widgets: inner.widgets.len(),
        }
    }

    /// Validate widget coverage and log warnings if coverage seems low.
    ///
    /// Call this at the end of your frame (after `capture_output()`) in
    /// debug builds to catch potential missing registrations.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    ///     // ... render widgets ...
    ///
    ///     self.bridge.capture_output(ctx);
    ///
    ///     #[cfg(debug_assertions)]
    ///     self.bridge.validate_coverage();
    /// }
    /// ```
    pub fn validate_coverage(&self) {
        let coverage = self.get_coverage();

        // Heuristic: if very few widgets registered and no AccessKit tree,
        // something is likely wrong
        if coverage.accesskit_nodes == 0 && coverage.registered_widgets == 0 {
            tracing::warn!(
                "MCP coverage warning: No widgets registered and no AccessKit tree. \
                 Did you forget to call capture_output() or register_widget()?"
            );
        } else if coverage.accesskit_nodes == 0 && coverage.registered_widgets < 3 {
            tracing::info!(
                "MCP coverage: {} widgets registered (no AccessKit tree). \
                 Consider using capture_output() for automatic coverage.",
                coverage.registered_widgets
            );
        }

        tracing::debug!(
            "MCP coverage: {} AccessKit nodes, {} manually registered widgets",
            coverage.accesskit_nodes,
            coverage.registered_widgets
        );
    }

    /// Inject any pending events into the egui context.
    /// Call this at the beginning of your update() method.
    /// Note: For better event injection, prefer using inject_raw_input() in raw_input_hook().
    pub fn inject_events(&self, ctx: &Context) {
        let mut inner = self.inner.lock().unwrap();

        // Get pending AccessKit actions and store them for later retrieval
        let actions = inner.event_queue.take_accesskit_actions();
        if !actions.is_empty() {
            let mut pending = self.pending_actions.lock().unwrap();
            pending.extend(actions);
        }

        // Inject regular egui events
        let events = inner.event_queue.take_egui_events();
        if !events.is_empty() {
            ctx.input_mut(|input| {
                for event in events {
                    input.events.push(event);
                }
            });
        }
    }

    /// Inject pending events into RawInput.
    /// Call this in raw_input_hook() for proper event timing.
    pub fn inject_raw_input(&self, raw_input: &mut egui::RawInput) {
        let mut inner = self.inner.lock().unwrap();

        // Process pending drag one phase per frame
        if let Some(ref mut drag) = inner.pending_drag {
            let event = match drag.phase {
                DragPhase::MoveToStart => {
                    drag.phase = DragPhase::Press;
                    Some(egui::Event::PointerMoved(drag.start_pos))
                }
                DragPhase::Press => {
                    drag.phase = DragPhase::Drag;
                    Some(egui::Event::PointerButton {
                        pos: drag.start_pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    })
                }
                DragPhase::Drag => {
                    drag.phase = DragPhase::Release;
                    Some(egui::Event::PointerMoved(drag.end_pos))
                }
                DragPhase::Release => {
                    drag.phase = DragPhase::Done;
                    Some(egui::Event::PointerButton {
                        pos: drag.end_pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    })
                }
                DragPhase::Done => None,
            };
            if let Some(event) = event {
                raw_input.events.push(event);
            }
        }
        // Clear completed drag
        if inner.pending_drag.as_ref().map(|d| d.phase == DragPhase::Done).unwrap_or(false) {
            inner.pending_drag = None;
        }

        // Process pending click one phase per frame (for buttons that need multi-frame clicks)
        if let Some(ref mut click) = inner.pending_click {
            let event = match click.phase {
                ClickPhase::Move => {
                    click.phase = ClickPhase::Press;
                    Some(egui::Event::PointerMoved(click.pos))
                }
                ClickPhase::Press => {
                    click.phase = ClickPhase::Release;
                    Some(egui::Event::PointerButton {
                        pos: click.pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    })
                }
                ClickPhase::Release => {
                    click.phase = ClickPhase::Done;
                    Some(egui::Event::PointerButton {
                        pos: click.pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    })
                }
                ClickPhase::Done => None,
            };
            if let Some(event) = event {
                raw_input.events.push(event);
            }
        }
        // Clear completed click
        if inner.pending_click.as_ref().map(|c| c.phase == ClickPhase::Done).unwrap_or(false) {
            inner.pending_click = None;
        }

        // Get pending AccessKit actions and store them for later retrieval
        let actions = inner.event_queue.take_accesskit_actions();
        if !actions.is_empty() {
            let mut pending = self.pending_actions.lock().unwrap();
            pending.extend(actions);
        }

        // Inject regular egui events into raw_input
        let events = inner.event_queue.take_egui_events();
        for event in events {
            raw_input.events.push(event);
        }
    }

    /// Get pending AccessKit action requests.
    /// The app should call this and process the actions appropriately.
    pub fn take_pending_actions(&self) -> Vec<ActionRequest> {
        let mut pending = self.pending_actions.lock().unwrap();
        std::mem::take(&mut *pending)
    }

    /// Update the AccessKit tree from a TreeUpdate.
    /// Call this when you receive an AccessKit update from egui.
    pub fn update_tree(&self, update: &TreeUpdate) {
        let mut inner = self.inner.lock().unwrap();
        inner.tree.update(update);
    }

    /// Capture the AccessKit tree from the platform output.
    /// Call this at the end of your update() method.
    /// Note: In egui 0.31+, you may need to use update_tree() directly
    /// with the AccessKit update from your platform integration.
    pub fn capture_output(&self, ctx: &Context) {
        // In egui 0.31+, the accesskit_update is accessed differently
        ctx.output(|output| {
            // Access accesskit_update if available
            if let Some(ref update) = output.accesskit_update {
                tracing::debug!(
                    "AccessKit update received with {} nodes",
                    update.nodes.len()
                );
                let mut inner = self.inner.lock().unwrap();
                inner.tree.update(update);
            }
        });
    }

    /// Process any pending commands from the MCP server.
    /// Call this in your update() method, typically after capture_output().
    pub fn process_commands(&self) {
        // Try to receive commands without blocking
        let mut rx = self.command_rx.lock().unwrap();

        while let Ok(command) = rx.try_recv() {
            self.handle_command(command);
        }
    }

    fn handle_command(&self, command: BridgeCommand) {
        let mut inner = self.inner.lock().unwrap();

        match command {
            BridgeCommand::GetSnapshot { respond } => {
                // Try AccessKit tree first, fall back to widget registry
                let (tree_str, node_count) = if inner.tree.node_count() > 0 {
                    (inner.tree.format_tree(), inner.tree.node_count())
                } else {
                    // Fall back to widget registry
                    let widgets: Vec<_> = inner.widgets.values().collect();
                    if widgets.is_empty() {
                        ("(no widgets registered - call bridge.register_widget() for each widget)".to_string(), 0)
                    } else {
                        let mut output = String::new();
                        let mut sorted_widgets: Vec<_> = widgets.into_iter().collect();
                        sorted_widgets.sort_by_key(|w| w.id);
                        for widget in sorted_widgets {
                            output.push_str(&format!(
                                "- {} \"{}\" [ref=n{}]",
                                widget.widget_type, widget.name, widget.id
                            ));
                            if let Some(ref value) = widget.value {
                                output.push_str(&format!(": \"{}\"", value));
                            }
                            if !widget.enabled {
                                output.push_str(" [disabled]");
                            }
                            output.push('\n');
                        }
                        (output, inner.widgets.len())
                    }
                };
                let response = SnapshotResponse {
                    tree: tree_str,
                    node_count,
                };
                self.runtime_handle.spawn(async move {
                    respond.send(response).await;
                });
            }

            BridgeCommand::Click { node_id, respond } => {
                // Try widget registry first (matches snapshot behavior)
                tracing::debug!("Click n{}: widgets count = {}", node_id.0, inner.widgets.len());
                if let Some(widget) = inner.widgets.get(&node_id.0) {
                    let center = widget.rect.center();
                    tracing::debug!("Click n{}: Found widget '{}' at ({}, {})",
                        node_id.0, widget.name, center.x, center.y);
                    // Use multi-frame click for reliable button activation
                    inner.pending_click = Some(PendingClick {
                        pos: center,
                        phase: ClickPhase::Move,
                    });
                    self.runtime_handle.spawn(async move {
                        respond.send(Ok(())).await;
                    });
                } else if let Some(node) = inner.tree.get(node_id) {
                    tracing::debug!("Click n{}: Not in widgets, trying AccessKit tree", node_id.0);
                    // Fall back to AccessKit tree
                    if let Some(center) = node.center() {
                        inner.pending_click = Some(PendingClick {
                            pos: center,
                            phase: ClickPhase::Move,
                        });
                    } else {
                        // No bounds available, try AccessKit action
                        inner.event_queue.queue_click(node_id);
                    }
                    self.runtime_handle.spawn(async move {
                        respond.send(Ok(())).await;
                    });
                } else {
                    let msg = format!("Node n{} not found", node_id.0);
                    self.runtime_handle.spawn(async move {
                        respond.send(Err(msg)).await;
                    });
                }
            }

            BridgeCommand::Focus { node_id, respond } => {
                // Try widget registry first (matches snapshot behavior)
                if let Some(widget) = inner.widgets.get(&node_id.0) {
                    let center = widget.rect.center();
                    inner.event_queue.queue_pointer_click(center);
                    self.runtime_handle.spawn(async move {
                        respond.send(Ok(())).await;
                    });
                } else if let Some(node) = inner.tree.get(node_id) {
                    // Fall back to AccessKit tree
                    if let Some(center) = node.center() {
                        inner.event_queue.queue_pointer_click(center);
                    } else {
                        inner.event_queue.queue_focus(node_id);
                    }
                    self.runtime_handle.spawn(async move {
                        respond.send(Ok(())).await;
                    });
                } else {
                    let msg = format!("Node n{} not found", node_id.0);
                    self.runtime_handle.spawn(async move {
                        respond.send(Err(msg)).await;
                    });
                }
            }

            BridgeCommand::SetValue {
                node_id,
                value,
                respond,
            } => {
                if inner.tree.get(node_id).is_some() {
                    inner.event_queue.queue_set_value(node_id, &value);
                    self.runtime_handle.spawn(async move {
                        respond.send(Ok(())).await;
                    });
                } else if let Some(widget) = inner.widgets.get(&node_id.0) {
                    if widget.widget_type == "slider" {
                        // For sliders, create a multi-frame drag operation
                        if let Ok(target_value) = value.parse::<f32>() {
                            // Get current value to calculate start position
                            let current_value = widget
                                .value
                                .as_ref()
                                .and_then(|v| v.parse::<f32>().ok())
                                .unwrap_or(50.0);

                            // The rect includes both slider rail and value display
                            // Estimate slider rail is about 65% of width (rest is value text box)
                            let rect = widget.rect;
                            let slider_rail_width = rect.width() * 0.65;
                            let y = rect.center().y;

                            // Calculate start and target x positions (assuming 0-100 range)
                            let start_x =
                                rect.left() + (current_value / 100.0) * slider_rail_width;
                            let target_x =
                                rect.left() + (target_value.clamp(0.0, 100.0) / 100.0) * slider_rail_width;

                            tracing::debug!(
                                "Slider drag: {} -> {}, pos ({:.1}, {:.1}) -> ({:.1}, {:.1})",
                                current_value, target_value, start_x, y, target_x, y
                            );

                            // Create a pending drag that will be processed over multiple frames
                            inner.pending_drag = Some(PendingDrag {
                                start_pos: egui::pos2(start_x, y),
                                end_pos: egui::pos2(target_x, y),
                                phase: DragPhase::MoveToStart,
                            });
                        }
                        self.runtime_handle.spawn(async move {
                            respond.send(Ok(())).await;
                        });
                    } else {
                        // For text inputs, click to focus then send text
                        let center = widget.rect.center();
                        inner.event_queue.queue_pointer_click(center);
                        // Clear existing text and type new value
                        inner.event_queue.queue_select_all();
                        inner.event_queue.queue_text(value);
                        self.runtime_handle.spawn(async move {
                            respond.send(Ok(())).await;
                        });
                    }
                } else {
                    let msg = format!("Node n{} not found", node_id.0);
                    self.runtime_handle.spawn(async move {
                        respond.send(Err(msg)).await;
                    });
                }
            }

            BridgeCommand::TypeText {
                node_id,
                text,
                respond,
            } => {
                if inner.tree.get(node_id).is_some() {
                    // Focus first, then type
                    inner.event_queue.queue_focus(node_id);
                    inner.event_queue.queue_text(text);
                    self.runtime_handle.spawn(async move {
                        respond.send(Ok(())).await;
                    });
                } else if let Some(widget) = inner.widgets.get(&node_id.0) {
                    // Click to focus, then type
                    let center = widget.rect.center();
                    inner.event_queue.queue_pointer_click(center);
                    inner.event_queue.queue_text(text);
                    self.runtime_handle.spawn(async move {
                        respond.send(Ok(())).await;
                    });
                } else {
                    let msg = format!("Node n{} not found", node_id.0);
                    self.runtime_handle.spawn(async move {
                        respond.send(Err(msg)).await;
                    });
                }
            }

            BridgeCommand::Hover { node_id, respond } => {
                // Try widget registry first (matches snapshot behavior)
                if let Some(widget) = inner.widgets.get(&node_id.0) {
                    let center = widget.rect.center();
                    inner.event_queue.queue_hover(center);
                    self.runtime_handle.spawn(async move {
                        respond.send(Ok(())).await;
                    });
                } else if let Some(node) = inner.tree.get(node_id) {
                    if let Some(center) = node.center() {
                        inner.event_queue.queue_hover(center);
                    }
                    // If no bounds, hover is a no-op but still succeeds
                    self.runtime_handle.spawn(async move {
                        respond.send(Ok(())).await;
                    });
                } else {
                    let msg = format!("Node n{} not found", node_id.0);
                    self.runtime_handle.spawn(async move {
                        respond.send(Err(msg)).await;
                    });
                }
            }

            BridgeCommand::GetValue { node_id, respond } => {
                // Try widget registry first (matches snapshot behavior)
                if let Some(widget) = inner.widgets.get(&node_id.0) {
                    let response = ValueResponse {
                        value: widget.value.clone(),
                        role: widget.widget_type.clone(),
                        name: Some(widget.name.clone()),
                    };
                    self.runtime_handle.spawn(async move {
                        respond.send(Ok(response)).await;
                    });
                } else if let Some(node) = inner.tree.get(node_id) {
                    let response = ValueResponse {
                        value: node.value.clone(),
                        role: format!("{:?}", node.role),
                        name: node.name.clone(),
                    };
                    self.runtime_handle.spawn(async move {
                        respond.send(Ok(response)).await;
                    });
                } else {
                    let msg = format!("Node n{} not found", node_id.0);
                    self.runtime_handle.spawn(async move {
                        respond.send(Err(msg)).await;
                    });
                }
            }

            BridgeCommand::Scroll {
                x,
                y,
                delta_x,
                delta_y,
                respond,
            } => {
                let pos = egui::pos2(x, y);
                let delta = egui::vec2(delta_x, delta_y);
                inner.event_queue.queue_scroll(pos, delta);
                self.runtime_handle.spawn(async move {
                    respond.send(Ok(())).await;
                });
            }
        }
    }
}
