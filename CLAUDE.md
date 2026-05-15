# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

egui-mcp is an MCP (Model Context Protocol) server and bridge library for E2E testing of egui applications. It enables AI assistants to interact with egui apps by:
1. Exposing the UI's AccessKit accessibility tree via MCP tools
2. Allowing programmatic interactions (clicks, text input, value changes)
3. **Managing app lifecycle** (launch, kill) with automatic display detection

## Build Commands

```bash
# Build all crates
cargo build

# Build release
cargo build --release

# Build and install MCP server
cargo build --release -p egui-mcp-server
# Binary: target/release/egui-mcp-server

# Run the test app (listens on port 9877)
cargo run -p test-app --features mcp

# Check without building
cargo check

# Run clippy
cargo clippy

# Format code
cargo fmt
```

## MCP Tools

### App Lifecycle
- `egui_launch` - Launch app with env vars, auto-connect (⭐ **pre-flight MCP check**, auto-detects X11)
- `egui_kill` - Kill launched app and disconnect
- `egui_connect` - Connect to already-running app
- `egui_disconnect` - Disconnect from app
- `egui_status` - Check connection and launch status

### UI Inspection & Interaction
- `egui_snapshot` - Get accessibility tree with `[ref=nX]` references
- `egui_click` - Click element by ref
- `egui_type` - Type text into input
- `egui_key` - Send keyboard event with optional modifiers (global, not widget-targeted). Use for shortcuts: `{key: "F", modifiers: ["ctrl"]}`, `{key: "Escape"}`, `{key: "F5"}`.
- `egui_fill` - Set value (sliders, spinboxes)
- `egui_focus` - Focus element
- `egui_hover` - Hover over element
- `egui_get_value` - Get element value
- `egui_scroll` - Scroll at position

## Virtual Display Testing (X11/Xvfb)

### Automatic X11 Detection

The `egui_launch` tool **automatically detects** when you're using a virtual display:

```
egui_launch({
  applicationPath: "./target/debug/my-app",
  args: ["file.txt"],
  env: { "DISPLAY": ":99" }  // ⭐ That's all you need!
})
```

When `DISPLAY` is set, it automatically:
1. Sets `WINIT_UNIX_BACKEND=x11` (forces X11 mode)
2. Removes `WAYLAND_DISPLAY` (prevents Wayland preference)

This ensures egui apps use the virtual X11 display on Wayland systems.

### Complete Virtual Display Workflow

```bash
# 1. Start Xvfb (once per session) - use helper function for reliability
start_xvfb() {
    pkill -9 Xvfb 2>/dev/null
    rm -f /tmp/.X99-lock /tmp/.X11-unix/X99
    Xvfb :99 -screen 0 1920x1080x24 &
    sleep 2
    DISPLAY=:99 xdpyinfo >/dev/null && echo "✅ Xvfb ready" || echo "❌ Xvfb failed"
}
start_xvfb
```

```
# 2. Launch app on virtual display (auto-detects X11)
# NOTE: egui_launch now verifies Xvfb is running before launching
egui_launch({
  applicationPath: "./target/debug/app",
  env: { "DISPLAY": ":99" }
})

# 3. Interact
egui_snapshot()
egui_click({ ref: "n5" })

# 4. Visual verification
screenshot_window({ pattern: "App", display: ":99" })

# 5. Cleanup
egui_kill()
```

### Pre-flight Checks

`egui_launch` now performs two pre-flight checks before launching:

1. **MCP Support Check**: Verifies binary has egui-mcp-bridge compiled in
2. **Xvfb Health Check**: Verifies X11 display :99 is responding (via `xdpyinfo`)

If Xvfb is not running (stale socket files), you'll get a helpful error:

```
❌ X11 display :99 is not responding.

Xvfb may have died leaving stale socket files.

To fix, run:
    pkill -9 Xvfb 2>/dev/null
    rm -f /tmp/.X99-lock /tmp/.X11-unix/X99
    Xvfb :99 -screen 0 1920x1080x24 &
    sleep 2
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
- Runs a TCP server (default port 9877) using JSON-RPC 2.0
- Captures AccessKit accessibility tree each frame
- Maintains widget registry for elements without AccessKit support
- Injects events (clicks, key presses, pointer events) into egui

**egui-mcp-server** (binary):
- MCP server using rmcp crate (stdio transport)
- Connects to egui-mcp-bridge via TCP
- Manages app lifecycle (launch/kill)
- Exposes all MCP tools listed above

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
- `crates/egui-mcp-server/src/tools.rs` - MCP tool implementations (including `egui_launch`)
- `crates/egui-mcp-server/src/bridge.rs` - TCP client to connect to bridge

### Integration Pattern

Apps integrate the bridge like this:

```rust
// Create bridge (starts TCP server)
let bridge = McpBridge::builder().port(9877).build();

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

## Testing Gotchas

### Binary Not Compiled with MCP Support

**Problem:** `egui_launch` fails with "MCP bridge not available" after waiting 10 seconds.

**Root cause:** The application binary was not compiled with the `mcp` feature flag.

**Solution:** The `egui_launch` tool now performs a **pre-flight check** before launching. If the binary lacks MCP support, you get an immediate error with fix instructions:

```
❌ Binary '/path/to/app' was NOT compiled with MCP bridge support.

The egui-mcp-bridge library is not linked into this binary.

To fix, rebuild with the 'mcp' feature enabled:

    cargo build --features mcp
```

**Manual check** (verify a binary has MCP support):
```bash
strings /path/to/binary | grep -q "MCP bridge listening" && echo "✅ Has MCP" || echo "❌ No MCP"
```

### Wayland vs X11 on Linux

**Problem:** egui/winit defaults to Wayland on Wayland systems, which ignores `DISPLAY` env var.

**Solution:** The `egui_launch` tool auto-detects this and forces X11 mode when `DISPLAY` is set.

**Manual workaround** (if not using `egui_launch`):
```bash
DISPLAY=:99 WINIT_UNIX_BACKEND=x11 ./my-app
```

### Xvfb Stale Socket Files (Screenshots Black/Empty)

**Problem:** `egui_launch` connects successfully, `egui_snapshot` works, but `screenshot_window` fails to find any windows and captures are black/empty.

**Root cause:** Xvfb process died but left stale socket files (`/tmp/.X99-lock`, `/tmp/.X11-unix/X99`). The egui MCP bridge works via TCP (doesn't need X11), but screenshots require a working X11 display.

**Symptoms:**
- `egui_launch` succeeds, `egui_snapshot` returns nodes
- `screenshot_window` says "Window not found"
- `xdotool search` returns nothing
- Screen captures are tiny (~400 bytes) black images

**Solution:** The `egui_launch` tool now verifies Xvfb is responding before launching. If you see the error, run:

```bash
pkill -9 Xvfb 2>/dev/null
rm -f /tmp/.X99-lock /tmp/.X11-unix/X99
Xvfb :99 -screen 0 1920x1080x24 &
sleep 2
```

**Prevention:** Always verify Xvfb before starting a test session:
```bash
DISPLAY=:99 xdpyinfo | head -3  # Should show "name of display: :99"
```

### Menu Bar Items Missing from Snapshots

**Problem:** Sub-buttons inside `ui.menu_button("File", |ui| { ... })` don't appear in `egui_snapshot()` output, so `egui_click` can't target them. Triggering the menu via keyboard works via `egui_key` (e.g. send `Alt+F` if the app wired it that way), but the menu items themselves stay invisible.

**Root cause:** egui's `MenuButton` widget doesn't emit AccessKit nodes for its sub-buttons — they only exist inside the closure's transient context. Upstream limitation, not an MCP bridge filter.

**Workaround:** Register menu items explicitly from inside the closure:

```rust
ui.menu_button("File", |ui| {
    let resp = ui.add(egui::Button::new("Find...").shortcut_text("Ctrl+F"));
    self.mcp_bridge.register_widget("File: Find...", "button", &resp, None);
    if resp.clicked() { /* ... */ }
});
```

The manual registration writes into the bridge's fallback widget registry, which `egui_snapshot` reads when AccessKit doesn't surface the node. After this, the menu item is clickable via `egui_click({ ref: "n<N>" })`.

**Alternative:** use `egui_key({ key: "F", modifiers: ["ctrl"] })` to fire the shortcut directly without opening the menu at all — usually the most natural test path.

### Port Conflicts

Default port is 9877. If multiple apps run simultaneously, specify different ports:

```rust
// In app
let bridge = McpBridge::builder().port(9878).build();
```

```
// In MCP
egui_launch({ applicationPath: "...", port: 9878 })
```
