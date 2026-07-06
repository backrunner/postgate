use crate::error::{PostGateError, Result};
use crate::rules::RuleGroup;
use crate::state::AppState;
use std::sync::Arc;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Manager, Runtime};

const TRAY_ID: &str = "postgate-main-tray";
const MENU_SHOW: &str = "postgate.tray.show";
const MENU_REFRESH: &str = "postgate.tray.refresh";
const MENU_QUIT: &str = "postgate.tray.quit";
const GROUP_ITEM_PREFIX: &str = "postgate.tray.rule-group.";

pub(crate) fn setup(app: &mut App, state: Arc<AppState>) -> tauri::Result<()> {
    let menu = build_menu(app.handle(), &[])?;
    let state_for_menu = Arc::clone(&state);

    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("PostGate")
        .show_menu_on_left_click(true)
        .on_menu_event(move |app_handle, event| {
            handle_menu_event(
                app_handle.clone(),
                Arc::clone(&state_for_menu),
                event.id().as_ref().to_string(),
            );
        });

    let icon = app
        .default_window_icon()
        .map(template_icon_from_app_icon)
        .unwrap_or_else(fallback_tray_icon_image);
    tray = tray.icon(icon).icon_as_template(true);

    tray.build(app)?;

    let app_handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = refresh(&app_handle, state).await {
            tracing::warn!("Failed to load tray rule groups: {}", error);
        }
    });

    Ok(())
}

pub(crate) async fn refresh(app: &AppHandle, state: Arc<AppState>) -> Result<()> {
    let groups = load_rule_groups(&state).await?;
    let menu = build_menu(app, &groups).map_err(|error| {
        PostGateError::InvalidState(format!("Failed to build tray menu: {}", error))
    })?;

    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(menu)).map_err(|error| {
            PostGateError::InvalidState(format!("Failed to set tray menu: {}", error))
        })?;
    }

    Ok(())
}

fn handle_menu_event(app: AppHandle, state: Arc<AppState>, id: String) {
    match id.as_str() {
        MENU_SHOW => show_main_window(&app),
        MENU_REFRESH => {
            tauri::async_runtime::spawn(async move {
                if let Err(error) = refresh(&app, state).await {
                    tracing::warn!("Failed to refresh tray rule groups: {}", error);
                }
            });
        }
        MENU_QUIT => app.exit(0),
        _ if id.starts_with(GROUP_ITEM_PREFIX) => {
            let group_id = id.trim_start_matches(GROUP_ITEM_PREFIX).to_string();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = toggle_rule_group_from_tray(state, &group_id).await {
                    tracing::warn!("Failed to toggle rule group from tray: {}", error);
                }
            });
        }
        _ => {}
    }
}

async fn toggle_rule_group_from_tray(state: Arc<AppState>, group_id: &str) -> Result<()> {
    load_rule_groups(&state).await?;

    let next_enabled = state
        .rule_engine
        .get_group(group_id)
        .map(|group| !group.enabled)
        .ok_or_else(|| PostGateError::NotFound(format!("Rule group '{}' not found", group_id)))?;

    if state.rule_engine.toggle_group(group_id, next_enabled) {
        if let Some(group) = state.rule_engine.get_group(group_id) {
            let db = state.get_database().await?;
            db.save_rule_group(&group).await?;
        }

        crate::rule_events::notify_rule_groups_changed(&state).await;
    }

    Ok(())
}

async fn load_rule_groups(state: &Arc<AppState>) -> Result<Vec<RuleGroup>> {
    let mut groups = state.rule_engine.get_all_groups();

    if groups.is_empty() {
        let db = state.get_database().await?;
        groups = db.get_rule_groups().await?;

        for group in &groups {
            state.rule_engine.upsert_group(group.clone());
        }
    }

    groups.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(groups)
}

fn build_menu<R: Runtime>(app: &AppHandle<R>, groups: &[RuleGroup]) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;

    let show_item = MenuItem::with_id(app, MENU_SHOW, "Show PostGate", true, None::<&str>)?;
    menu.append(&show_item)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    let title_item = MenuItem::with_id(
        app,
        "postgate.tray.rules",
        "Rule Groups",
        false,
        None::<&str>,
    )?;
    menu.append(&title_item)?;

    if groups.is_empty() {
        let empty_item = MenuItem::with_id(
            app,
            "postgate.tray.empty",
            "No rule groups",
            false,
            None::<&str>,
        )?;
        menu.append(&empty_item)?;
    } else {
        for group in groups {
            let item = CheckMenuItem::with_id(
                app,
                format!("{}{}", GROUP_ITEM_PREFIX, group.id),
                menu_text(&group.name),
                true,
                group.enabled,
                None::<&str>,
            )?;
            menu.append(&item)?;
        }
    }

    menu.append(&PredefinedMenuItem::separator(app)?)?;
    let refresh_item =
        MenuItem::with_id(app, MENU_REFRESH, "Refresh Rule Groups", true, None::<&str>)?;
    menu.append(&refresh_item)?;

    menu.append(&PredefinedMenuItem::separator(app)?)?;
    let quit_item = MenuItem::with_id(app, MENU_QUIT, "Quit PostGate", true, None::<&str>)?;
    menu.append(&quit_item)?;

    Ok(menu)
}

fn menu_text(name: &str) -> String {
    let trimmed = name.trim();
    let text = if trimmed.is_empty() {
        "Untitled Rule Group"
    } else {
        trimmed
    };

    text.replace('&', "&&")
}

fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        tracing::warn!("Main window not found for tray show action");
        return;
    };

    if let Err(error) = window.show() {
        tracing::warn!("Failed to show main window from tray: {}", error);
    }

    if let Err(error) = window.set_focus() {
        tracing::warn!("Failed to focus main window from tray: {}", error);
    }
}

fn template_icon_from_app_icon(source: &Image<'_>) -> Image<'static> {
    const SIZE: u32 = 32;
    let source_width = source.width();
    let source_height = source.height();

    if source_width == 0 || source_height == 0 {
        return fallback_tray_icon_image();
    }

    let scale = (SIZE as f32 / source_width as f32).min(SIZE as f32 / source_height as f32);
    let output_width = (source_width as f32 * scale)
        .round()
        .clamp(1.0, SIZE as f32) as u32;
    let output_height = (source_height as f32 * scale)
        .round()
        .clamp(1.0, SIZE as f32) as u32;
    let offset_x = (SIZE - output_width) / 2;
    let offset_y = (SIZE - output_height) / 2;

    let mut rgba = vec![0; (SIZE * SIZE * 4) as usize];
    for y in 0..output_height {
        for x in 0..output_width {
            let source_x = ((x as f32 + 0.5) / scale - 0.5).clamp(0.0, source_width as f32 - 1.0);
            let source_y = ((y as f32 + 0.5) / scale - 0.5).clamp(0.0, source_height as f32 - 1.0);
            let alpha = sample_template_alpha(source, source_x, source_y);
            set_alpha(&mut rgba, SIZE, offset_x + x, offset_y + y, alpha);
        }
    }

    Image::new_owned(rgba, SIZE, SIZE)
}

fn sample_template_alpha(source: &Image<'_>, x: f32, y: f32) -> u8 {
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(source.width() - 1);
    let y1 = (y0 + 1).min(source.height() - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;

    let top = lerp(
        pixel_template_alpha(source, x0, y0) as f32,
        pixel_template_alpha(source, x1, y0) as f32,
        tx,
    );
    let bottom = lerp(
        pixel_template_alpha(source, x0, y1) as f32,
        pixel_template_alpha(source, x1, y1) as f32,
        tx,
    );

    lerp(top, bottom, ty).round().clamp(0.0, 255.0) as u8
}

fn pixel_template_alpha(source: &Image<'_>, x: u32, y: u32) -> u8 {
    let index = ((y * source.width() + x) * 4) as usize;
    let rgba = source.rgba();
    let red = rgba[index] as f32;
    let green = rgba[index + 1] as f32;
    let blue = rgba[index + 2] as f32;
    let source_alpha = rgba[index + 3] as f32 / 255.0;
    let luminance = 0.2126 * red + 0.7152 * green + 0.0722 * blue;
    let glyph_alpha = ((luminance - 34.0) / 180.0).clamp(0.0, 1.0);

    (glyph_alpha * source_alpha * 255.0).round() as u8
}

fn lerp(start: f32, end: f32, amount: f32) -> f32 {
    start + (end - start) * amount
}

fn fallback_tray_icon_image() -> Image<'static> {
    const SIZE: u32 = 32;
    let mut rgba = vec![0; (SIZE * SIZE * 4) as usize];

    fill_rect(&mut rgba, SIZE, 8, 7, 16, 4, 255);
    fill_rect(&mut rgba, SIZE, 8, 7, 4, 19, 255);
    fill_rect(&mut rgba, SIZE, 20, 7, 4, 19, 255);
    fill_rect(&mut rgba, SIZE, 12, 14, 8, 3, 255);
    fill_rect(&mut rgba, SIZE, 12, 20, 8, 3, 255);

    // Slightly soften the outer corners so the template icon scales cleanly
    // in the macOS menu bar without relying on a PNG asset pipeline.
    set_alpha(&mut rgba, SIZE, 8, 7, 160);
    set_alpha(&mut rgba, SIZE, 23, 7, 160);
    set_alpha(&mut rgba, SIZE, 8, 25, 160);
    set_alpha(&mut rgba, SIZE, 23, 25, 160);

    Image::new_owned(rgba, SIZE, SIZE)
}

fn fill_rect(rgba: &mut [u8], size: u32, x: u32, y: u32, width: u32, height: u32, alpha: u8) {
    for row in y..(y + height) {
        for col in x..(x + width) {
            set_alpha(rgba, size, col, row, alpha);
        }
    }
}

fn set_alpha(rgba: &mut [u8], size: u32, x: u32, y: u32, alpha: u8) {
    let index = ((y * size + x) * 4) as usize;
    rgba[index] = 0;
    rgba[index + 1] = 0;
    rgba[index + 2] = 0;
    rgba[index + 3] = alpha;
}
