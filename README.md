# egui-mcp

MCP (Model Context Protocol) server for E2E testing egui applications.

## Features

- 🤖 **AI-Powered Testing** - Test egui apps using natural language via Claude Code
- 🎯 **AccessKit Integration** - Automatic UI tree inspection
- 🖱️ **Full Interaction** - Click, type, fill, hover, scroll
- 🚀 **App Lifecycle Management** - Launch, kill, connect
- 🖥️ **Virtual Display Support** - Auto-detects X11 mode for Xvfb testing
- 📸 **Visual Verification** - Combine with screenshot tools

## Quick Start

### 1. Add MCP Bridge to Your App

```toml
[dependencies]
egui-mcp-bridge = { path = "path/to/egui-mcp/crates/egui-mcp-bridge" }

[features]
mcp = ["egui-mcp-bridge"]
```

```rust
use egui_mcp_bridge::McpBridge;

struct MyApp {
    #[cfg(feature = "mcp")]
    bridge: McpBridge,
}

impl eframe::App for MyApp {
    fn raw_input_hook(&mut self, _ctx: &Context, raw_input: &mut RawInput) {
        #[cfg(feature = "mcp")]
        {
            self.bridge.process_commands();
            self.bridge.inject_raw_input(raw_input);
        }
    }

    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        #[cfg(feature = "mcp")]
        {
            ctx.enable_accesskit();
            self.bridge.begin_frame();
        }

        // Your UI code...
        let btn = ui.button("Click Me");
        
        #[cfg(feature = "mcp")]
        self.bridge.register_widget("Click Me", "button", &btn, None);

        #[cfg(feature = "mcp")]
        self.bridge.capture_output(ctx);
    }
}
```

### 2. Add MCP Server to Claude Code

```bash
# Build release binary
cd egui-mcp
cargo build --release -p egui-mcp-server

# Add to Claude Code
claude mcp add egui ~/egui-mcp/target/release/egui-mcp-server -s user
```

### 3. Test Your App

```
# Launch app on virtual display
egui_launch({
  applicationPath: "./target/debug/my-app",
  args: ["file.txt"],
  env: { "DISPLAY": ":99" }  // Auto-detects X11 mode!
})

# Inspect UI
egui_snapshot()

# Interact
egui_click({ ref: "n5" })
egui_type({ ref: "n3", text: "Hello" })

# Cleanup
egui_kill()
```

## Virtual Display Testing

For isolated testing that doesn't interfere with your desktop:

```bash
# Start Xvfb (once)
Xvfb :99 -screen 0 1920x1080x24 &

# Launch app (auto-detects X11 mode)
egui_launch({
  applicationPath: "./target/debug/my-app",
  env: { "DISPLAY": ":99" }
})
```

**The tool automatically:**
- Sets `WINIT_UNIX_BACKEND=x11` when DISPLAY is specified
- Removes `WAYLAND_DISPLAY` to prevent Wayland preference
- Ensures egui apps use the virtual X11 display on Wayland systems

## Available Tools

| Tool | Description |
|------|-------------|
| `egui_launch` | Launch app with env vars, auto-connect |
| `egui_kill` | Kill launched app and disconnect |
| `egui_connect` | Connect to already-running app |
| `egui_disconnect` | Disconnect from app |
| `egui_status` | Check connection status |
| `egui_snapshot` | Get accessibility tree |
| `egui_click` | Click element by ref |
| `egui_type` | Type text into input |
| `egui_fill` | Set value (sliders, etc.) |
| `egui_focus` | Focus element |
| `egui_hover` | Hover over element |
| `egui_get_value` | Get element value |
| `egui_scroll` | Scroll at position |

## Architecture

```
┌─────────────┐     stdio      ┌──────────────────┐     TCP/JSON-RPC    ┌────────────────┐
│ Claude Code │ ◄──────────────► │ egui-mcp-server  │ ◄──────────────────► │ egui-mcp-bridge│
│   (Client)  │                 │   (MCP Server)   │                      │  (in egui app) │
└─────────────┘                 └──────────────────┘                      └────────────────┘
```

- **egui-mcp-bridge** - Rust library embedded in your egui app
- **egui-mcp-server** - Standalone MCP server binary

## Development

```bash
# Build all
cargo build

# Build release
cargo build --release

# Run test app
cargo run -p test-app --features mcp

# Run clippy
cargo clippy

# Format
cargo fmt
```

## License

MIT OR Apache-2.0
