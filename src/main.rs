use niri_ipc::socket::Socket;
use niri_ipc::{Event, Request, Response, Window, Workspace};
use std::collections::HashMap;
use std::fs;

fn main() {
    loop {
        if let Err(e) = run_daemon() {
            eprintln!("Daemon error: {}, reconnecting in 1s...", e);
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}

fn run_daemon() -> Result<(), Box<dyn std::error::Error>> {
    // Get initial state and output
    let (workspaces, windows) = fetch_state()?;
    output_status(&workspaces, &windows);

    // Subscribe to event stream
    let mut socket = Socket::connect()?;
    let reply = socket.send(Request::EventStream)?;

    match reply {
        Ok(Response::Handled) => {}
        Ok(other) => return Err(format!("Unexpected response: {:?}", other).into()),
        Err(msg) => return Err(msg.into()),
    }

    let mut read_event = socket.read_events();

    loop {
        let event = read_event()?;

        // Only update on relevant events
        match event {
            Event::WorkspacesChanged { .. }
            | Event::WorkspaceActivated { .. }
            | Event::WindowsChanged { .. }
            | Event::WindowOpenedOrChanged { .. }
            | Event::WindowClosed { .. }
            | Event::WindowFocusChanged { .. } => {
                if let Ok((ws, win)) = fetch_state() {
                    output_status(&ws, &win);
                }
            }
            _ => {}
        }
    }
}

fn fetch_state() -> Result<(Vec<Workspace>, Vec<Window>), Box<dyn std::error::Error>> {
    let mut socket = Socket::connect()?;

    let workspaces = match socket.send(Request::Workspaces)? {
        Ok(Response::Workspaces(ws)) => ws,
        Ok(other) => return Err(format!("Unexpected: {:?}", other).into()),
        Err(msg) => return Err(msg.into()),
    };

    let mut socket = Socket::connect()?;
    let windows = match socket.send(Request::Windows)? {
        Ok(Response::Windows(ws)) => ws,
        Ok(other) => return Err(format!("Unexpected: {:?}", other).into()),
        Err(msg) => return Err(msg.into()),
    };

    Ok((workspaces, windows))
}

fn output_status(workspaces: &[Workspace], windows: &[Window]) {
    // Pre-compute terminal apps
    let terminal_apps: HashMap<i32, &str> = windows
        .iter()
        .filter(|w| is_terminal(w.app_id.as_deref().unwrap_or("")))
        .filter_map(|w| w.pid.map(|pid| (pid, detect_terminal_app(pid as u32))))
        .filter(|(_, app)| !app.is_empty())
        .collect();

    let mut sorted_workspaces: Vec<&Workspace> = workspaces.iter().collect();
    sorted_workspaces.sort_by_key(|w| w.idx);

    let mut before_focused: Vec<String> = Vec::new();
    let mut after_focused: Vec<String> = Vec::new();
    let mut focused_output = String::new();
    let mut current_section = "before";
    let mut chrome_idx = 0;
    let mut tooltip = String::new();

    for ws in &sorted_workspaces {
        let mut ws_windows: Vec<&Window> = windows
            .iter()
            .filter(|w| w.workspace_id == Some(ws.id))
            .collect();

        ws_windows.sort_by_key(|win| window_sort_key(*win));

        let win_count = ws_windows.len();
        tooltip.push_str(&format!("ws{}: {} windows\\n", ws.idx, win_count));

        let ws_output = if win_count == 0 {
            if ws.is_focused {
                "<span color='#888888'>█</span>".to_string()
            } else {
                "<span color='#444444'>·</span>".to_string()
            }
        } else {
            let mut output = String::new();
            for win in &ws_windows {
                let app_id = win.app_id.as_deref().unwrap_or("");
                let title = win.title.as_deref().unwrap_or("");
                let mut color = get_color(app_id, title, win.pid, &terminal_apps);

                if color == "chrome" {
                    const CHROME_COLORS: [&str; 4] = ["#ea4335", "#fbbc05", "#34a853", "#4285f4"];
                    color = CHROME_COLORS[chrome_idx % 4].to_string();
                    chrome_idx += 1;
                }

                if !ws.is_focused {
                    color = dim_color(&color);
                }

                let bar = if win.is_focused {
                    "█"
                } else if !ws.is_focused && ws.active_window_id == Some(win.id) {
                    "▌"
                } else {
                    "|"
                };

                output.push_str(&format!("<span color='{}'>{}</span>", color, bar));
            }
            output
        };

        if ws.is_focused {
            focused_output = ws_output;
            current_section = "after";
        } else if current_section == "before" {
            before_focused.push(ws_output);
        } else {
            after_focused.push(ws_output);
        }
    }

    let mut output = String::new();
    for ws in &before_focused {
        output.push_str(ws);
        output.push_str("  ");
    }
    output.push_str(&focused_output);
    for ws in &after_focused {
        output.push_str("  ");
        output.push_str(ws);
    }

    if tooltip.ends_with("\\n") {
        tooltip.truncate(tooltip.len() - 2);
    }

    println!(r#"{{"text": "{}", "tooltip": "{}"}}"#, output, tooltip);
}

fn is_terminal(app_id: &str) -> bool {
    app_id == "Alacritty" || app_id == "kitty" || app_id == "com.mitchellh.ghostty"
}

fn detect_terminal_app(pid: u32) -> &'static str {
    if let Ok(children) = get_all_descendants(pid) {
        for child_pid in children {
            if let Ok(cmdline) = fs::read_to_string(format!("/proc/{}/cmdline", child_pid)) {
                let cmdline = cmdline.to_lowercase();
                if cmdline.contains("claude") {
                    return "claude";
                } else if cmdline.contains("codex") {
                    return "codex";
                } else if cmdline.contains("nvim") || cmdline.contains("vim") {
                    return "nvim";
                }
            }
        }
    }
    ""
}

fn get_all_descendants(pid: u32) -> std::io::Result<Vec<u32>> {
    let mut descendants = Vec::new();
    let mut to_check = vec![pid];

    while let Some(current) = to_check.pop() {
        let children_path = format!("/proc/{}/task/{}/children", current, current);
        if let Ok(children_str) = fs::read_to_string(&children_path) {
            for child in children_str.split_whitespace() {
                if let Ok(child_pid) = child.parse::<u32>() {
                    descendants.push(child_pid);
                    to_check.push(child_pid);
                }
            }
        }
    }

    Ok(descendants)
}

fn get_color(app_id: &str, title: &str, pid: Option<i32>, terminal_apps: &HashMap<i32, &str>) -> String {
    if is_terminal(app_id) {
        let title_lower = title.to_lowercase();

        if title_lower.contains("claude") {
            return "#f5a623".to_string();
        } else if title_lower.contains("codex") {
            return "#56b6c2".to_string();
        } else if title_lower.contains("nvim") || title_lower.contains("vim") {
            return "#98c379".to_string();
        }

        if let Some(pid) = pid {
            if let Some(&app) = terminal_apps.get(&pid) {
                return match app {
                    "claude" => "#f5a623",
                    "codex" => "#56b6c2",
                    "nvim" => "#98c379",
                    _ => "#888888",
                }
                .to_string();
            }
        }

        return "#888888".to_string();
    }

    if app_id.contains("nvim") {
        return "#98c379".to_string();
    }
    if app_id == "google-chrome" || app_id.contains("chrome") {
        return "chrome".to_string();
    }
    if app_id == "firefox" {
        return "#ff7139".to_string();
    }
    if app_id == "vesktop" || app_id == "discord" {
        return "#c678dd".to_string();
    }
    if app_id.contains("todoist") {
        return "#e06c75".to_string();
    }
    if app_id.contains("mail.google") {
        return "#e5c07b".to_string();
    }

    "#666666".to_string()
}

fn dim_color(hex: &str) -> String {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return format!("#{}", hex);
    }

    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);

    let r = ((r as u32 * 6 / 10) + 30).min(255) as u8;
    let g = ((g as u32 * 6 / 10) + 30).min(255) as u8;
    let b = ((b as u32 * 6 / 10) + 30).min(255) as u8;

    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

fn window_sort_key(win: &Window) -> (u8, u64, u64, u64) {
    if let Some((col, row)) = win.layout.pos_in_scrolling_layout {
        (0, col as u64, row as u64, win.id)
    } else {
        // Floating/unknown layout windows go after tiled windows.
        (1, 0, 0, win.id)
    }
}
