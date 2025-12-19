use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::process::Command;

#[derive(Deserialize)]
struct Workspace {
    id: i64,
    idx: i32,
    is_focused: bool,
    active_window_id: Option<i64>,
}

#[derive(Deserialize)]
struct Window {
    id: i64,
    workspace_id: i64,
    app_id: String,
    title: String,
    is_focused: bool,
    pid: Option<u32>,
}

fn main() {
    // Get data from niri in parallel
    let ws_handle = std::thread::spawn(|| {
        Command::new("niri")
            .args(["msg", "-j", "workspaces"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
    });

    let win_handle = std::thread::spawn(|| {
        Command::new("niri")
            .args(["msg", "-j", "windows"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
    });

    let (workspaces_json, windows_json) = match (ws_handle.join().ok().flatten(), win_handle.join().ok().flatten()) {
        (Some(w), Some(win)) => (w, win),
        _ => {
            println!(r#"{{"text": "", "tooltip": "niri not running"}}"#);
            return;
        }
    };

    let mut workspaces: Vec<Workspace> = serde_json::from_str(&workspaces_json).unwrap_or_default();
    let windows: Vec<Window> = serde_json::from_str(&windows_json).unwrap_or_default();

    workspaces.sort_by_key(|w| w.idx);

    // Pre-compute terminal apps by checking /proc
    let terminal_apps: HashMap<u32, &str> = windows
        .iter()
        .filter(|w| is_terminal(&w.app_id))
        .filter_map(|w| w.pid)
        .map(|pid| (pid, detect_terminal_app(pid)))
        .filter(|(_, app)| !app.is_empty())
        .collect();

    let mut before_focused: Vec<String> = Vec::new();
    let mut after_focused: Vec<String> = Vec::new();
    let mut focused_output = String::new();
    let mut current_section = "before";
    let mut chrome_idx = 0;
    let mut tooltip = String::new();

    for ws in &workspaces {
        let mut ws_windows: Vec<&Window> = windows
            .iter()
            .filter(|w| w.workspace_id == ws.id)
            .collect();

        ws_windows.sort_by_key(|w| w.id);

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
                let mut color = get_color(&win.app_id, &win.title, win.pid, &terminal_apps);

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
                } else if !ws.is_focused && Some(win.id) == ws.active_window_id {
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

fn get_color(app_id: &str, title: &str, pid: Option<u32>, terminal_apps: &HashMap<u32, &str>) -> String {
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
                }.to_string();
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
