//! Cross-platform native startup window and WebView handoff.
//!
//! The startup window is a Tauri native window, not a WebView.  Keeping the
//! configured WebView disabled until initialization completes lets the event
//! loop paint this window before the browser engine is created.

use std::{path::Path, time::Duration};

use tauri::{Manager, Theme, WebviewWindowBuilder, window::Color};

const STARTUP_WINDOW_LABEL: &str = "startup";
const MAIN_WINDOW_LABEL: &str = "main";
const THEME_FILE_NAME: &str = "startup-theme";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StartupPalette {
    background: Color,
    theme: Theme,
}

impl StartupPalette {
    fn for_id(id: &str) -> Option<Self> {
        let (background, theme) = match id {
            "default" => ([0x09, 0x0d, 0x18], Theme::Dark),
            "catppuccin-latte" => ([0xef, 0xf1, 0xf5], Theme::Light),
            "catppuccin-frappe" => ([0x30, 0x34, 0x46], Theme::Dark),
            "catppuccin-macchiato" => ([0x24, 0x27, 0x3a], Theme::Dark),
            "catppuccin-mocha" => ([0x1e, 0x1e, 0x2e], Theme::Dark),
            "dracula" => ([0x21, 0x22, 0x2c], Theme::Dark),
            "nord-dark" => ([0x24, 0x29, 0x33], Theme::Dark),
            "nord-light" => ([0xec, 0xef, 0xf4], Theme::Light),
            "gruvbox-dark" => ([0x1d, 0x20, 0x21], Theme::Dark),
            "gruvbox-light" => ([0xf7, 0xed, 0xc2], Theme::Light),
            "tokyo-night" => ([0x16, 0x16, 0x1e], Theme::Dark),
            "tokyo-day" => ([0xe9, 0xe9, 0xec], Theme::Light),
            "rose-pine" => ([0x19, 0x17, 0x24], Theme::Dark),
            "rose-pine-dawn" => ([0xf8, 0xf1, 0xe9], Theme::Light),
            "solarized-dark" => ([0x00, 0x2b, 0x36], Theme::Dark),
            "solarized-light" => ([0xf8, 0xf1, 0xdc], Theme::Light),
            "github-dark" => ([0x0d, 0x11, 0x17], Theme::Dark),
            "github-light" => ([0xf6, 0xf8, 0xfa], Theme::Light),
            "kanagawa" => ([0x16, 0x16, 0x1d], Theme::Dark),
            "everforest" => ([0x27, 0x2e, 0x33], Theme::Dark),
            _ => return None,
        };
        Some(Self {
            background: Color(background[0], background[1], background[2], 255),
            theme,
        })
    }

    fn read(data_dir: &Path) -> Self {
        std::fs::read_to_string(data_dir.join(THEME_FILE_NAME))
            .ok()
            .and_then(|id| Self::for_id(id.trim()))
            .unwrap_or_else(|| {
                Self::for_id("default").unwrap_or(Self {
                    background: Color(0x09, 0x0d, 0x18, 255),
                    theme: Theme::Dark,
                })
            })
    }
}

pub fn show_native(app: &tauri::App, data_dir: &Path) -> tauri::Result<()> {
    let palette = StartupPalette::read(data_dir);
    tauri::window::WindowBuilder::new(app, STARTUP_WINDOW_LABEL)
        .title("App Tester · Starting…")
        .inner_size(440.0, 230.0)
        .center()
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .closable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .theme(Some(palette.theme))
        .background_color(palette.background)
        .build()?;
    Ok(())
}

pub fn create_main_webview(app: &tauri::AppHandle) -> tauri::Result<()> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN_WINDOW_LABEL)
        .cloned()
        .ok_or(tauri::Error::WindowNotFound)?;
    WebviewWindowBuilder::from_config(app, &config)?.build()?;

    let fallback = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(8)).await;
        let _ = reveal_main(&fallback);
    });
    Ok(())
}

fn reveal_main(app: &tauri::AppHandle) -> Result<(), String> {
    let main = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "main window is not available".to_owned())?;
    if main.is_visible().map_err(|error| error.to_string())? {
        if let Some(startup) = app.get_window(STARTUP_WINDOW_LABEL) {
            startup.close().map_err(|error| error.to_string())?;
        }
        return Ok(());
    }
    main.show().map_err(|error| error.to_string())?;
    main.set_focus().map_err(|error| error.to_string())?;
    if let Some(startup) = app.get_window(STARTUP_WINDOW_LABEL) {
        startup.close().map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn frontend_ready(app: tauri::AppHandle) -> Result<(), String> {
    reveal_main(&app)
}

#[tauri::command]
pub fn remember_startup_theme(app: tauri::AppHandle, theme: String) -> Result<(), String> {
    if StartupPalette::for_id(&theme).is_none() {
        return Err("unknown color theme".to_owned());
    }
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
    std::fs::write(data_dir.join(THEME_FILE_NAME), theme).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_every_supported_palette_and_rejects_unknown_values() {
        for id in [
            "default",
            "catppuccin-latte",
            "catppuccin-frappe",
            "catppuccin-macchiato",
            "catppuccin-mocha",
            "dracula",
            "nord-dark",
            "nord-light",
            "gruvbox-dark",
            "gruvbox-light",
            "tokyo-night",
            "tokyo-day",
            "rose-pine",
            "rose-pine-dawn",
            "solarized-dark",
            "solarized-light",
            "github-dark",
            "github-light",
            "kanagawa",
            "everforest",
        ] {
            assert!(StartupPalette::for_id(id).is_some(), "missing {id}");
        }
        assert!(StartupPalette::for_id("unknown").is_none());
    }

    #[test]
    fn light_and_dark_native_chrome_follow_the_palette() {
        assert_eq!(
            StartupPalette::for_id("catppuccin-latte").map(|palette| palette.theme),
            Some(Theme::Light)
        );
        assert_eq!(
            StartupPalette::for_id("catppuccin-mocha").map(|palette| palette.theme),
            Some(Theme::Dark)
        );
    }
}
