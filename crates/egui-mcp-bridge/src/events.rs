//! Event injection for egui via AccessKit actions.

use accesskit::{Action, ActionRequest, NodeId};
use egui::{Event, Pos2};
use std::collections::VecDeque;

/// Queue of events to inject into egui.
#[derive(Debug, Default)]
pub struct EventQueue {
    accesskit_actions: VecDeque<ActionRequest>,
    pointer_events: VecDeque<PointerEvent>,
    text_events: VecDeque<TextEvent>,
    key_events: VecDeque<KeyEvent>,
}

/// Key event to inject.
#[derive(Debug, Clone)]
pub struct KeyEvent {
    pub key: egui::Key,
    pub pressed: bool,
    pub modifiers: egui::Modifiers,
}

/// Pointer event to inject.
#[derive(Debug, Clone)]
pub struct PointerEvent {
    pub kind: PointerEventKind,
    pub pos: Option<Pos2>,
}

#[derive(Debug, Clone)]
pub enum PointerEventKind {
    Move,
    Press,
    Release,
}

/// Text input event.
#[derive(Debug, Clone)]
pub struct TextEvent {
    pub text: String,
}

impl EventQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a click action on a node.
    pub fn queue_click(&mut self, node_id: NodeId) {
        self.accesskit_actions.push_back(ActionRequest {
            target: node_id,
            action: Action::Click,
            data: None,
        });
    }

    /// Queue a focus action on a node.
    pub fn queue_focus(&mut self, node_id: NodeId) {
        self.accesskit_actions.push_back(ActionRequest {
            target: node_id,
            action: Action::Focus,
            data: None,
        });
    }

    /// Queue a set value action on a node.
    pub fn queue_set_value(&mut self, node_id: NodeId, value: &str) {
        self.accesskit_actions.push_back(ActionRequest {
            target: node_id,
            action: Action::SetValue,
            data: Some(accesskit::ActionData::Value(value.into())),
        });
    }

    /// Queue text input (will be injected after focusing).
    pub fn queue_text(&mut self, text: String) {
        self.text_events.push_back(TextEvent { text });
    }

    /// Queue a select-all keyboard shortcut (Ctrl+A).
    pub fn queue_select_all(&mut self) {
        self.key_events.push_back(KeyEvent {
            key: egui::Key::A,
            pressed: true,
            modifiers: egui::Modifiers::COMMAND, // Ctrl on Linux/Windows, Cmd on Mac
        });
        self.key_events.push_back(KeyEvent {
            key: egui::Key::A,
            pressed: false,
            modifiers: egui::Modifiers::COMMAND,
        });
    }

    /// Queue a pointer move (hover).
    pub fn queue_hover(&mut self, pos: Pos2) {
        self.pointer_events.push_back(PointerEvent {
            kind: PointerEventKind::Move,
            pos: Some(pos),
        });
    }

    /// Queue a pointer click (press + release) at a position.
    pub fn queue_pointer_click(&mut self, pos: Pos2) {
        self.pointer_events.push_back(PointerEvent {
            kind: PointerEventKind::Press,
            pos: Some(pos),
        });
        self.pointer_events.push_back(PointerEvent {
            kind: PointerEventKind::Release,
            pos: Some(pos),
        });
    }

    /// Queue a slider drag from start to end position.
    pub fn queue_slider_drag(&mut self, from: Pos2, to: Pos2) {
        // Move to start position
        self.pointer_events.push_back(PointerEvent {
            kind: PointerEventKind::Move,
            pos: Some(from),
        });
        // Press at start
        self.pointer_events.push_back(PointerEvent {
            kind: PointerEventKind::Press,
            pos: Some(from),
        });
        // Drag to end position
        self.pointer_events.push_back(PointerEvent {
            kind: PointerEventKind::Move,
            pos: Some(to),
        });
        // Release at end
        self.pointer_events.push_back(PointerEvent {
            kind: PointerEventKind::Release,
            pos: Some(to),
        });
    }

    /// Take all pending AccessKit action requests.
    pub fn take_accesskit_actions(&mut self) -> Vec<ActionRequest> {
        self.accesskit_actions.drain(..).collect()
    }

    /// Take all pending egui events.
    pub fn take_egui_events(&mut self) -> Vec<Event> {
        let mut events = Vec::new();

        // Convert pointer events
        for pe in self.pointer_events.drain(..) {
            if let Some(pos) = pe.pos {
                match pe.kind {
                    PointerEventKind::Move => {
                        events.push(Event::PointerMoved(pos));
                    }
                    PointerEventKind::Press => {
                        events.push(Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                    PointerEventKind::Release => {
                        events.push(Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                }
            }
        }

        // Convert key events
        for ke in self.key_events.drain(..) {
            events.push(Event::Key {
                key: ke.key,
                physical_key: None,
                pressed: ke.pressed,
                repeat: false,
                modifiers: ke.modifiers,
            });
        }

        // Convert text events
        for te in self.text_events.drain(..) {
            events.push(Event::Text(te.text));
        }

        events
    }

    /// Check if there are any pending events.
    pub fn has_pending(&self) -> bool {
        !self.accesskit_actions.is_empty()
            || !self.pointer_events.is_empty()
            || !self.text_events.is_empty()
            || !self.key_events.is_empty()
    }
}
