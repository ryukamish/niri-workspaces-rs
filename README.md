# niri-workspaces-rs

A fast, event-driven workspace indicator for [Waybar](https://github.com/Alexays/Waybar) and [niri](https://github.com/YaLTeR/niri).

<img width="719" height="1762" alt="image" src="https://github.com/user-attachments/assets/d9f11bb1-1902-4ce5-ad1f-b56756b0d259" />

## Features

- **Visual bars** instead of numbers - each window is a colored bar
- **Color-coded by app** - Chrome (Google colors), Discord (purple), Firefox (orange), nvim (green), Claude (orange), Codex (cyan)
- **Terminal app detection** - detects Claude/Codex/nvim running inside terminals via /proc
- **Focused window indicator** - █ for focused, ▌ for would-be-focused, | for others
- **Dimmed unfocused workspaces** - colors are dimmed when workspace isn't focused
- **Daemon mode** - persistent socket connection, no process spawning

## Benchmarks

| Metric | Value |
|--------|-------|
| Memory | 2.5 MB RSS |
| CPU per update | ~1.1ms |
| 80 rapid switches | ~90ms total CPU |

### Comparison

| Approach | CPU per update |
|----------|---------------|
| Bash script | ~340ms |
| Rust + signal | ~12-14ms |
| Rust daemon | **~1.1ms** |

The daemon is **~10-12x faster** than the signal-based approach and **~300x faster** than the original bash script.

## Installation

```bash
cargo build --release
cp target/release/niri-workspaces ~/.config/waybar/
```

## Waybar Configuration

```json
"custom/workspaces": {
    "exec": "/home/user/.config/waybar/niri-workspaces",
    "return-type": "json",
    "format": "{}"
}
```

No `interval` or `signal` needed - the daemon outputs JSON whenever workspace/window events occur.

## License

MIT
