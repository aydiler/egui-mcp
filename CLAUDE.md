# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

egui-mcp is an MCP (Model Context Protocol) server and bridge library for E2E testing of egui applications. It enables AI assistants to interact with egui apps by:
1. Exposing the UI's AccessKit accessibility tree via MCP tools
2. Allowing programmatic interactions (clicks, text input, value changes)

## Build Commands

```bash
# Build all crates
cargo build

# Build release
cargo build --release

# Run the MCP server (communicates via stdio)
cargo run -p egui-mcp-server

# Run the test app (listens on port 9876)
cargo run -p test-app

# Check without building
cargo check

# Run clippy
cargo clippy

# Format code
cargo fmt
```

## Architecture

### Workspace Structure

```
egui-mcp/
├── crates/
│   ├── egui-mcp-bridge/    # Library embedded in egui apps
│   └── egui-mcp-server/    # MCP server binary
└── examples/
    └── test-app/           # Example egui app with bridge integration
```

### Two-Component Architecture

**egui-mcp-bridge** (library):
- Embedded into egui applications
- Runs a TCP server (default port 9876) using JSON-RPC 2.0
- Captures AccessKit accessibility tree each frame
- Maintains widget registry for elements without AccessKit support
- Injects events (clicks, key presses, pointer events) into egui

**egui-mcp-server** (binary):
- MCP server using rmcp crate (stdio transport)
- Connects to egui-mcp-bridge via TCP
- Exposes tools: `egui_connect`, `egui_snapshot`, `egui_click`, `egui_type`, `egui_fill`, `egui_focus`, `egui_hover`, `egui_get_value`

### Communication Flow

```
MCP Client (Claude) <--stdio--> egui-mcp-server <--TCP/JSON-RPC--> egui-mcp-bridge <--> egui App
```

### Key Source Files

- `crates/egui-mcp-bridge/src/lib.rs` - Main `McpBridge` type, command handling, event injection
- `crates/egui-mcp-bridge/src/server.rs` - TCP server and JSON-RPC request handling
- `crates/egui-mcp-bridge/src/tree.rs` - AccessKit tree serialization with `[ref=nX]` format
- `crates/egui-mcp-bridge/src/events.rs` - Event queue for injecting pointer/key/text events
- `crates/egui-mcp-bridge/src/protocol.rs` - JSON-RPC protocol types
- `crates/egui-mcp-server/src/tools.rs` - MCP tool implementations
- `crates/egui-mcp-server/src/bridge.rs` - TCP client to connect to bridge

### Integration Pattern

Apps integrate the bridge like this:

```rust
// Create bridge (starts TCP server)
let bridge = McpBridge::builder().port(9876).build();

// In eframe::App implementation:
fn raw_input_hook(&mut self, _ctx: &Context, raw_input: &mut RawInput) {
    self.bridge.process_commands();
    self.bridge.inject_raw_input(raw_input);
}

fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
    ctx.enable_accesskit();
    self.bridge.begin_frame();

    // ... UI code with optional widget registration ...
    let response = ui.button("Click Me");
    self.bridge.register_widget("Click Me", "button", &response, None);

    self.bridge.capture_output(ctx);
}
```

### Node References

Elements are identified by refs like `[ref=n3]`. The number comes from:
- AccessKit `NodeId` (preferred, automatic)
- Widget registry counter (fallback for manual registration)

### Multi-Frame Operations

Slider drags use a state machine (`DragPhase`) that executes over multiple frames:
`MoveToStart` -> `Press` -> `Drag` -> `Release` -> `Done`
