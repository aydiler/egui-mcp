//! Event injection for egui via AccessKit actions.

use egui::accesskit::{Action, ActionRequest, NodeId};
use egui::{Event, Pos2, Vec2};
use std::collections::VecDeque;

/// Queue of events to inject into egui.
#[derive(Debug, Default)]
pub struct EventQueue {
    accesskit_actions: VecDeque<ActionRequest>,
    pointer_events: VecDeque<PointerEvent>,
    text_events: VecDeque<TextEvent>,
    key_events: VecDeque<KeyEvent>,
    scroll_events: VecDeque<ScrollEvent>,
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

/// Scroll event.
#[derive(Debug, Clone)]
pub struct ScrollEvent {
    pub pos: Pos2,
    pub delta: Vec2,
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
            data: Some(egui::accesskit::ActionData::Value(value.into())),
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

    /// Queue a key press, optionally followed by a release.
    ///
    /// Most app keyboard handlers use `Input::key_pressed(K)`, which only fires on
    /// the press transition. Emitting both press + release matches realistic input
    /// and keeps `Input::keys_down` clean across frames. Use `press_only = true` to
    /// hold a key (e.g. for chord tests that release later).
    pub fn queue_key(&mut self, key: egui::Key, modifiers: egui::Modifiers, press_only: bool) {
        self.key_events.push_back(KeyEvent {
            key,
            pressed: true,
            modifiers,
        });
        if !press_only {
            self.key_events.push_back(KeyEvent {
                key,
                pressed: false,
                modifiers,
            });
        }
    }

    /// Queue a pointer move (hover).
    pub fn queue_hover(&mut self, pos: Pos2) {
        self.pointer_events.push_back(PointerEvent {
            kind: PointerEventKind::Move,
            pos: Some(pos),
        });
    }

    /// Queue a pointer click (move + press + release) at a position.
    pub fn queue_pointer_click(&mut self, pos: Pos2) {
        // First move pointer to position (required for egui to detect hover)
        self.pointer_events.push_back(PointerEvent {
            kind: PointerEventKind::Move,
            pos: Some(pos),
        });
        self.pointer_events.push_back(PointerEvent {
            kind: PointerEventKind::Press,
            pos: Some(pos),
        });
        self.pointer_events.push_back(PointerEvent {
            kind: PointerEventKind::Release,
            pos: Some(pos),
        });
    }

    /// Queue a scroll event at a position.
    pub fn queue_scroll(&mut self, pos: Pos2, delta: Vec2) {
        // First move pointer to position so scroll happens in the right area
        self.pointer_events.push_back(PointerEvent {
            kind: PointerEventKind::Move,
            pos: Some(pos),
        });
        self.scroll_events.push_back(ScrollEvent { pos, delta });
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

        // Convert scroll events
        for se in self.scroll_events.drain(..) {
            events.push(Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: se.delta,
                modifiers: egui::Modifiers::NONE,
            });
        }

        events
    }

    /// Check if there are any pending events.
    pub fn has_pending(&self) -> bool {
        !self.accesskit_actions.is_empty()
            || !self.pointer_events.is_empty()
            || !self.text_events.is_empty()
            || !self.key_events.is_empty()
            || !self.scroll_events.is_empty()
    }
}

/// Parse a key-name string into an `egui::Key`. Case-insensitive.
///
/// Supported categories: letters A–Z, digits 0–9 (mapped to `Num0`–`Num9`),
/// function keys F1–F35, named keys (Enter, Escape, Tab, Space, Backspace,
/// Delete, Home, End, PageUp, PageDown, Insert), arrows (ArrowLeft / Right /
/// Up / Down), and common punctuation symbols.
pub fn parse_key(name: &str) -> Result<egui::Key, String> {
    use egui::Key;
    let lc = name.to_ascii_lowercase();

    // Single-letter A–Z
    if lc.len() == 1 {
        if let Some(c) = lc.chars().next() {
            match c {
                'a' => return Ok(Key::A),
                'b' => return Ok(Key::B),
                'c' => return Ok(Key::C),
                'd' => return Ok(Key::D),
                'e' => return Ok(Key::E),
                'f' => return Ok(Key::F),
                'g' => return Ok(Key::G),
                'h' => return Ok(Key::H),
                'i' => return Ok(Key::I),
                'j' => return Ok(Key::J),
                'k' => return Ok(Key::K),
                'l' => return Ok(Key::L),
                'm' => return Ok(Key::M),
                'n' => return Ok(Key::N),
                'o' => return Ok(Key::O),
                'p' => return Ok(Key::P),
                'q' => return Ok(Key::Q),
                'r' => return Ok(Key::R),
                's' => return Ok(Key::S),
                't' => return Ok(Key::T),
                'u' => return Ok(Key::U),
                'v' => return Ok(Key::V),
                'w' => return Ok(Key::W),
                'x' => return Ok(Key::X),
                'y' => return Ok(Key::Y),
                'z' => return Ok(Key::Z),
                '0' => return Ok(Key::Num0),
                '1' => return Ok(Key::Num1),
                '2' => return Ok(Key::Num2),
                '3' => return Ok(Key::Num3),
                '4' => return Ok(Key::Num4),
                '5' => return Ok(Key::Num5),
                '6' => return Ok(Key::Num6),
                '7' => return Ok(Key::Num7),
                '8' => return Ok(Key::Num8),
                '9' => return Ok(Key::Num9),
                _ => {}
            }
        }
    }

    // Function keys F1–F35
    if let Some(rest) = lc.strip_prefix('f') {
        if let Ok(n) = rest.parse::<u8>() {
            return match n {
                1 => Ok(Key::F1),
                2 => Ok(Key::F2),
                3 => Ok(Key::F3),
                4 => Ok(Key::F4),
                5 => Ok(Key::F5),
                6 => Ok(Key::F6),
                7 => Ok(Key::F7),
                8 => Ok(Key::F8),
                9 => Ok(Key::F9),
                10 => Ok(Key::F10),
                11 => Ok(Key::F11),
                12 => Ok(Key::F12),
                13 => Ok(Key::F13),
                14 => Ok(Key::F14),
                15 => Ok(Key::F15),
                16 => Ok(Key::F16),
                17 => Ok(Key::F17),
                18 => Ok(Key::F18),
                19 => Ok(Key::F19),
                20 => Ok(Key::F20),
                21 => Ok(Key::F21),
                22 => Ok(Key::F22),
                23 => Ok(Key::F23),
                24 => Ok(Key::F24),
                25 => Ok(Key::F25),
                26 => Ok(Key::F26),
                27 => Ok(Key::F27),
                28 => Ok(Key::F28),
                29 => Ok(Key::F29),
                30 => Ok(Key::F30),
                31 => Ok(Key::F31),
                32 => Ok(Key::F32),
                33 => Ok(Key::F33),
                34 => Ok(Key::F34),
                35 => Ok(Key::F35),
                _ => Err(format!(
                    "Function key out of range: '{}' (supported F1–F35)",
                    name
                )),
            };
        }
    }

    // Named keys
    match lc.as_str() {
        "enter" | "return" => Ok(Key::Enter),
        "escape" | "esc" => Ok(Key::Escape),
        "tab" => Ok(Key::Tab),
        "space" | "spacebar" => Ok(Key::Space),
        "backspace" => Ok(Key::Backspace),
        "delete" | "del" => Ok(Key::Delete),
        "home" => Ok(Key::Home),
        "end" => Ok(Key::End),
        "pageup" | "page_up" => Ok(Key::PageUp),
        "pagedown" | "page_down" => Ok(Key::PageDown),
        "insert" | "ins" => Ok(Key::Insert),
        "arrowleft" | "arrow_left" | "left" => Ok(Key::ArrowLeft),
        "arrowright" | "arrow_right" | "right" => Ok(Key::ArrowRight),
        "arrowup" | "arrow_up" | "up" => Ok(Key::ArrowUp),
        "arrowdown" | "arrow_down" | "down" => Ok(Key::ArrowDown),
        "plus" => Ok(Key::Plus),
        "minus" => Ok(Key::Minus),
        "equals" | "equal" => Ok(Key::Equals),
        "comma" => Ok(Key::Comma),
        "period" | "dot" => Ok(Key::Period),
        "slash" => Ok(Key::Slash),
        "backslash" => Ok(Key::Backslash),
        "semicolon" => Ok(Key::Semicolon),
        "quote" => Ok(Key::Quote),
        "openbracket" | "open_bracket" | "leftbracket" => Ok(Key::OpenBracket),
        "closebracket" | "close_bracket" | "rightbracket" => Ok(Key::CloseBracket),
        "colon" => Ok(Key::Colon),
        "pipe" => Ok(Key::Pipe),
        "questionmark" | "question" => Ok(Key::Questionmark),
        "backtick" | "grave" => Ok(Key::Backtick),
        _ => Err(format!(
            "Unknown key: '{}'. Supported: A–Z, 0–9, F1–F35, Enter, Escape, Tab, \
             Space, Backspace, Delete, Home, End, PageUp, PageDown, Insert, \
             ArrowLeft/Right/Up/Down, Plus, Minus, Equals, Comma, Period, Slash, \
             Backslash, Semicolon, Quote, OpenBracket, CloseBracket, Colon, Pipe, \
             Questionmark, Backtick.",
            name
        )),
    }
}

/// Parse a list of modifier-name strings (case-insensitive) into `egui::Modifiers`.
///
/// Supported names: `ctrl`, `shift`, `alt`, `command`. `command` maps to
/// `Modifiers::command` which egui resolves to Ctrl on Linux/Windows and Cmd on Mac.
pub fn parse_modifiers(names: &[String]) -> Result<egui::Modifiers, String> {
    let mut mods = egui::Modifiers::NONE;
    for name in names {
        match name.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods.ctrl = true,
            "shift" => mods.shift = true,
            "alt" => mods.alt = true,
            "command" | "cmd" => mods.command = true,
            other => {
                return Err(format!(
                    "Unknown modifier: '{}'. Supported: ctrl, shift, alt, command.",
                    other
                ));
            }
        }
    }
    Ok(mods)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_key_letters_case_insensitive() {
        assert_eq!(parse_key("F").unwrap(), egui::Key::F);
        assert_eq!(parse_key("f").unwrap(), egui::Key::F);
        assert_eq!(parse_key("a").unwrap(), egui::Key::A);
        assert_eq!(parse_key("Z").unwrap(), egui::Key::Z);
    }

    #[test]
    fn parse_key_digits_map_to_num() {
        assert_eq!(parse_key("0").unwrap(), egui::Key::Num0);
        assert_eq!(parse_key("9").unwrap(), egui::Key::Num9);
    }

    #[test]
    fn parse_key_function_keys() {
        assert_eq!(parse_key("F1").unwrap(), egui::Key::F1);
        assert_eq!(parse_key("f5").unwrap(), egui::Key::F5);
        assert_eq!(parse_key("F12").unwrap(), egui::Key::F12);
        assert_eq!(parse_key("F35").unwrap(), egui::Key::F35);
        assert!(parse_key("F36").is_err());
        assert!(parse_key("F0").is_err());
    }

    #[test]
    fn parse_key_named() {
        assert_eq!(parse_key("Enter").unwrap(), egui::Key::Enter);
        assert_eq!(parse_key("Return").unwrap(), egui::Key::Enter);
        assert_eq!(parse_key("Escape").unwrap(), egui::Key::Escape);
        assert_eq!(parse_key("Esc").unwrap(), egui::Key::Escape);
        assert_eq!(parse_key("Space").unwrap(), egui::Key::Space);
        assert_eq!(parse_key("Tab").unwrap(), egui::Key::Tab);
    }

    #[test]
    fn parse_key_arrows() {
        assert_eq!(parse_key("ArrowLeft").unwrap(), egui::Key::ArrowLeft);
        assert_eq!(parse_key("left").unwrap(), egui::Key::ArrowLeft);
        assert_eq!(parse_key("Right").unwrap(), egui::Key::ArrowRight);
    }

    #[test]
    fn parse_key_unknown_returns_err() {
        let err = parse_key("Squirrel").unwrap_err();
        assert!(err.contains("Unknown key"));
        assert!(err.contains("Squirrel"));
    }

    #[test]
    fn parse_modifiers_empty_is_none() {
        let m = parse_modifiers(&[]).unwrap();
        assert!(!m.ctrl && !m.shift && !m.alt && !m.command);
    }

    #[test]
    fn parse_modifiers_ctrl_shift() {
        let m = parse_modifiers(&["ctrl".into(), "shift".into()]).unwrap();
        assert!(m.ctrl);
        assert!(m.shift);
        assert!(!m.alt);
        assert!(!m.command);
    }

    #[test]
    fn parse_modifiers_case_insensitive_and_aliases() {
        let m = parse_modifiers(&["CTRL".into(), "Cmd".into()]).unwrap();
        assert!(m.ctrl);
        assert!(m.command);
    }

    #[test]
    fn parse_modifiers_unknown_returns_err() {
        let err = parse_modifiers(&["meta".into()]).unwrap_err();
        assert!(err.contains("Unknown modifier"));
    }

    #[test]
    fn queue_key_emits_press_and_release_by_default() {
        let mut q = EventQueue::new();
        q.queue_key(egui::Key::F, egui::Modifiers::CTRL, false);
        let events = q.take_egui_events();
        assert_eq!(events.len(), 2);
        match &events[0] {
            egui::Event::Key {
                key,
                pressed,
                modifiers,
                ..
            } => {
                assert_eq!(*key, egui::Key::F);
                assert!(*pressed);
                assert!(modifiers.ctrl);
            }
            other => panic!("expected Key event, got {:?}", other),
        }
        match &events[1] {
            egui::Event::Key { key, pressed, .. } => {
                assert_eq!(*key, egui::Key::F);
                assert!(!*pressed);
            }
            other => panic!("expected Key release, got {:?}", other),
        }
    }

    #[test]
    fn queue_key_press_only_emits_single_event() {
        let mut q = EventQueue::new();
        q.queue_key(egui::Key::Escape, egui::Modifiers::NONE, true);
        let events = q.take_egui_events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            egui::Event::Key { pressed, .. } => assert!(*pressed),
            other => panic!("expected Key event, got {:?}", other),
        }
    }
}
