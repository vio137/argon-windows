/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;
#[cfg(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
use std::fs;
#[cfg(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use accesskit::Affine;
use dpi::PhysicalSize;
use egui::text::{CCursor, CCursorRange};
use egui::text_edit::TextEditState;
use egui::{
    Button, FontDefinitions, Id, Key, Label, LayerId, Modifiers, Order, PaintCallback, Panel, Vec2,
    WidgetInfo, WidgetType, pos2,
};
#[cfg(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
use egui::{FontData, FontFamily};
use egui_glow::{CallbackFn, EguiGlow};
use egui_winit::EventResponse;
use euclid::{Length, Point2D, Rect, Scale, Size2D};
#[cfg(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
use log::info;
use log::warn;
use servo::{
    DeviceIndependentPixel, DevicePixel, Image, LoadStatus, OffscreenRenderingContext, PixelFormat,
    RenderingContext, WebView, WebViewId,
};
use url::Url;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::Window;

use crate::desktop::event_loop::AppEvent;
use crate::desktop::headed_window;
use crate::running_app_state::{RunningAppState, UserInterfaceCommand};
use crate::window::{ServoShellWindow, TopLevelWebViewCreationRequest};

/// Argon chrome settings, persisted as JSON in the user's config directory so
/// choices survive restarts. Every control in the settings window applies and
/// saves immediately - no decorative, unsaved state.
#[derive(Clone)]
struct ArgonSettings {
    /// Zen's signature dark chrome (true) or the light variant (false).
    dark_theme: bool,
    /// URL opened by the Home action (toolbar context menu).
    homepage: String,
    /// Start with the tab sidebar in compact (icon-strip) mode.
    sidebar_compact: bool,
}

impl Default for ArgonSettings {
    fn default() -> Self {
        Self {
            dark_theme: true,
            homepage: "https://servo.org".into(),
            sidebar_compact: false,
        }
    }
}

impl ArgonSettings {
    fn path() -> Option<std::path::PathBuf> {
        dirs::config_dir().map(|dir| dir.join("argon").join("settings.json"))
    }

    fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            return Self::default();
        };
        let mut settings = Self::default();
        if let Some(dark) = value.get("dark_theme").and_then(|v| v.as_bool()) {
            settings.dark_theme = dark;
        }
        if let Some(homepage) = value.get("homepage").and_then(|v| v.as_str()) &&
            !homepage.is_empty()
        {
            settings.homepage = homepage.to_string();
        }
        if let Some(compact) = value.get("sidebar_compact").and_then(|v| v.as_bool()) {
            settings.sidebar_compact = compact;
        }
        settings
    }

    fn save(&self) {
        let Some(path) = Self::path() else {
            return;
        };
        if let Some(parent) = path.parent() &&
            let Err(error) = std::fs::create_dir_all(parent)
        {
            warn!("Could not create Argon settings directory: {error}.");
            return;
        }
        let json = serde_json::json!({
            "dark_theme": self.dark_theme,
            "homepage": self.homepage,
            "sidebar_compact": self.sidebar_compact,
        });
        if let Err(error) = std::fs::write(&path, json.to_string()) {
            warn!("Could not save Argon settings: {error}.");
        }
    }
}

/// The user interface of a headed servoshell. Currently this is implemented via
/// egui.
pub struct Gui {
    rendering_context: Rc<OffscreenRenderingContext>,
    context: EguiGlow,
    toolbar_height: Length<f32, DeviceIndependentPixel>,

    /// The current width of the Zen-style tab sidebar (zero when hidden).
    sidebar_width: Length<f32, DeviceIndependentPixel>,

    /// Whether the tab sidebar is collapsed to a compact strip (Zen-style compact mode).
    sidebar_compact: bool,

    /// Zen-style pinned tabs ("essentials"): shown as favicon-only rows at the top of the
    /// tab sidebar. Chrome-level state, not persisted across restarts yet.
    pinned_tabs: std::collections::HashSet<WebViewId>,

    /// Argon chrome settings (persisted); edited in the settings window.
    settings: ArgonSettings,

    /// Whether the settings window is open.
    settings_open: bool,

    /// Position for the webview context menu (window coordinates), when open.
    context_menu_pos: Option<egui::Pos2>,

    location: String,

    /// Whether the location has been edited by the user without clicking Go.
    location_dirty: bool,

    /// The [`LoadStatus`] of the active `WebView`.
    load_status: LoadStatus,

    /// The text to display in the status bar on the bottom of the window.
    status_text: Option<String>,

    /// Whether or not the current `WebView` can navigate backward.
    can_go_back: bool,

    /// Whether or not the current `WebView` can navigate forward.
    can_go_forward: bool,

    /// Handle to the GPU texture of the favicon.
    ///
    /// These need to be cached across egui draw calls.
    favicon_textures: HashMap<WebViewId, (egui::TextureHandle, egui::load::SizedTexture)>,

    /// AccessKit tree updates pending the next egui tick.
    /// This allows us to ensure that graft nodes are sent before the subtrees they graft.
    pending_accesskit_updates: Vec<accesskit::TreeUpdate>,
}

fn truncate_with_ellipsis(input: &str, max_length: usize) -> String {
    if input.chars().count() > max_length {
        let truncated: String = input.chars().take(max_length.saturating_sub(1)).collect();
        format!("{}…", truncated)
    } else {
        input.to_string()
    }
}

/// Visual style for the Argon chrome, modeled on Zen browser's design language
/// (`--zen-border-radius: 7px`, quiet surfaces, fills derived from a single accent).
fn zen_visuals(mut visuals: egui::Visuals) -> egui::Visuals {
    use egui::{Color32, CornerRadius};

    let (surface, extreme_bg, faint_bg, hover_bg, active_bg, selection_bg, text, weak_text) =
        if visuals.dark_mode {
            (
                Color32::from_rgb(20, 19, 22),    // toolbar/sidebar surface
                Color32::from_rgb(11, 10, 11),    // Zen dark base (--zen-primary-color dark)
                Color32::from_rgb(27, 26, 30),    // input background
                Color32::from_rgb(35, 34, 40),    // hover
                Color32::from_rgb(48, 46, 56),    // active
                Color32::from_rgb(54, 56, 92),    // accent (80, 90, 200) mixed into the base
                Color32::from_rgb(235, 235, 235),
                Color32::from_rgb(160, 159, 166),
            )
        } else {
            (
                Color32::from_rgb(245, 244, 247),
                Color32::from_rgb(255, 255, 255),
                Color32::from_rgb(237, 236, 241),
                Color32::from_rgb(228, 227, 235),
                Color32::from_rgb(213, 212, 226),
                Color32::from_rgb(198, 202, 240), // accent (100, 115, 255) mixed into the base
                Color32::from_rgb(24, 23, 27),
                Color32::from_rgb(105, 104, 112),
            )
        };

    visuals.window_fill = surface;
    visuals.panel_fill = surface;
    visuals.extreme_bg_color = extreme_bg;
    visuals.faint_bg_color = faint_bg;
    visuals.text_edit_bg_color = Some(faint_bg);
    visuals.widgets.noninteractive.weak_bg_fill = surface;
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(7);
    visuals.widgets.noninteractive.fg_stroke.color = weak_text;
    visuals.widgets.inactive.weak_bg_fill = hover_bg;
    visuals.widgets.inactive.corner_radius = CornerRadius::same(7);
    visuals.widgets.inactive.fg_stroke.color = text;
    visuals.widgets.hovered.weak_bg_fill = hover_bg;
    visuals.widgets.hovered.corner_radius = CornerRadius::same(7);
    visuals.widgets.hovered.fg_stroke.color = text;
    visuals.widgets.active.weak_bg_fill = active_bg;
    visuals.widgets.active.corner_radius = CornerRadius::same(7);
    visuals.widgets.active.fg_stroke.color = text;
    visuals.selection.bg_fill = selection_bg;
    visuals
}

#[cfg(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
fn load_cjk_fonts(font_candidates: &[(&str, &str)]) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let mut loaded_font_names = Vec::new();

    for (path_str, font_name) in font_candidates.iter() {
        let font_path = Path::new(path_str);
        if font_path.exists() {
            match fs::read(font_path) {
                Ok(bytes) => {
                    if !fonts.font_data.contains_key(*font_name) {
                        fonts
                            .font_data
                            .insert(font_name.to_string(), Arc::new(FontData::from_owned(bytes)));
                        loaded_font_names.push(font_name.to_string());
                        info!("Loaded font: {}", font_name);
                    }
                },
                Err(error) => {
                    info!("Failed to read font {}: {}", font_name, error);
                },
            }
        }
    }

    if !loaded_font_names.is_empty() {
        let proportional = fonts.families.get_mut(&FontFamily::Proportional).unwrap();
        for font_name in loaded_font_names.iter() {
            proportional.insert(0, font_name.clone());
        }
    }

    fonts
}

#[cfg(target_os = "windows")]
fn configure_fonts() -> FontDefinitions {
    load_cjk_fonts(&[
        (r"C:\Windows\Fonts\malgun.ttf", "Malgun Gothic"), // Korean
        (r"C:\Windows\Fonts\msyh.ttc", "Microsoft YaHei"), // Chinese + Japanese
        (r"C:\Windows\Fonts\segoeui.ttf", "Segoe UI"), // Native Windows interface
    ])
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn configure_fonts() -> FontDefinitions {
    load_cjk_fonts(&[
        (
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "Noto Sans CJK",
        ), // Ubuntu/Debian
        (
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "Noto Sans CJK",
        ), // Fedora/Arch
        // FreeBSD splits the Noto CJK fonts into regional subsets
        (
            "/usr/local/share/fonts/noto/NotoSansCJKhk-Regular.otf",
            "Noto Sans CJK HK",
        ),
        (
            "/usr/local/share/fonts/noto/NotoSansCJKjp-Regular.otf",
            "Noto Sans CJK JP",
        ),
        (
            "/usr/local/share/fonts/noto/NotoSansCJKkr-Regular.otf",
            "Noto Sans CJK KR",
        ),
        (
            "/usr/local/share/fonts/noto/NotoSansCJKsc-Regular.otf",
            "Noto Sans CJK SC",
        ),
        (
            "/usr/local/share/fonts/noto/NotoSansCJKtc-Regular.otf",
            "Noto Sans CJK TC",
        ),
        (
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "WenQuanYi Micro Hei",
        ), // common fallback
        (
            "/usr/local/share/fonts/wqy/wqy-microhei.ttc",
            "WenQuanYi Micro Hei",
        ), // FreeBSD
    ])
}

#[cfg(target_os = "macos")]
fn configure_fonts() -> FontDefinitions {
    load_cjk_fonts(&[
        (
            "/System/Library/Fonts/AppleSDGothicNeo.ttc",
            "Apple SD Gothic Neo",
        ), // Korean
        ("/System/Library/Fonts/STHeiti Medium.ttc", "STHeiti Medium"), // Chinese + Japanese
    ])
}

impl Drop for Gui {
    fn drop(&mut self) {
        self.rendering_context
            .make_current()
            .expect("Could not make window RenderingContext current");
        self.context.destroy();
    }
}

impl Gui {
    pub(crate) fn new(
        winit_window: &Window,
        event_loop: &ActiveEventLoop,
        event_loop_proxy: EventLoopProxy<AppEvent>,
        rendering_context: Rc<OffscreenRenderingContext>,
        initial_url: Url,
    ) -> Self {
        rendering_context
            .make_current()
            .expect("Could not make window RenderingContext current");
        let mut context = EguiGlow::new(
            event_loop,
            rendering_context.glow_gl_api(),
            None,
            None,
            false,
        );

        let font_definitions = configure_fonts();
        context.egui_ctx.set_fonts(font_definitions);

        context
            .egui_winit
            .init_accesskit(event_loop, winit_window, event_loop_proxy);

        // Argon chrome styling, modeled on Zen browser's design language.
        context
            .egui_ctx
            .set_visuals_of(egui::Theme::Dark, zen_visuals(egui::Visuals::dark()));
        context
            .egui_ctx
            .set_visuals_of(egui::Theme::Light, zen_visuals(egui::Visuals::light()));

        winit_window.set_visible(true);

        // Apply the persisted theme choice (Zen dark is the default).
        let settings = ArgonSettings::load();
        context.egui_ctx.set_theme(if settings.dark_theme {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        });

        context.egui_ctx.options_mut(|options| {
            // Disable the builtin egui handlers for the Ctrl+Plus, Ctrl+Minus and Ctrl+0
            // shortcuts as they don't work well with servoshell's `device-pixel-ratio` CLI argument.
            options.zoom_with_keyboard = false;

            // Argon follows Zen's signature dark chrome by default; on platforms
            // where winit fails to obtain a system theme, fall back to dark too.
            options.fallback_theme = egui::Theme::Dark;
        });

        Self {
            rendering_context,
            context,
            toolbar_height: Default::default(),
            sidebar_width: Default::default(),
            sidebar_compact: settings.sidebar_compact,
            settings,
            settings_open: false,
            context_menu_pos: None,
            pinned_tabs: Default::default(),
            location: initial_url.to_string(),
            location_dirty: false,
            load_status: LoadStatus::Complete,
            status_text: None,
            can_go_back: false,
            can_go_forward: false,
            favicon_textures: Default::default(),
            pending_accesskit_updates: vec![],
        }
    }

    pub(crate) fn has_keyboard_focus(&self) -> bool {
        self.context
            .egui_ctx
            .memory(|memory| memory.focused().is_some())
    }

    pub(crate) fn surrender_focus(&self) {
        self.context.egui_ctx.memory_mut(|memory| {
            if let Some(focused) = memory.focused() {
                memory.surrender_focus(focused);
            }
        });
    }

    /// Open the chrome context menu at the given window position, in response to
    /// a secondary click over the webview (intercepted in `headed_window`).
    pub(crate) fn open_context_menu(&mut self, position: Point2D<f32, DeviceIndependentPixel>) {
        self.context_menu_pos = Some(egui::pos2(position.x, position.y));
        self.context.egui_ctx.request_repaint();
    }

    pub(crate) fn on_window_event(
        &mut self,
        winit_window: &Window,
        event: &WindowEvent,
    ) -> EventResponse {
        self.context.on_window_event(winit_window, event)
    }

    /// The height of the top toolbar of this user inteface ie the distance from the top of the
    /// window to the position of the `WebView`.
    pub(crate) fn toolbar_height(&self) -> Length<f32, DeviceIndependentPixel> {
        self.toolbar_height
    }

    /// The offset from the top-left corner of the window to the top-left corner of the
    /// `WebView`, ie the size of the Argon chrome (top toolbar and tab sidebar).
    pub(crate) fn chrome_offset(&self) -> Point2D<f32, DeviceIndependentPixel> {
        Point2D::new(self.sidebar_width.get(), self.toolbar_height.get())
    }

    /// Return true iff the given position is over the egui chrome (toolbar or tab sidebar).
    pub(crate) fn is_in_egui_chrome_rect(
        &self,
        position: Point2D<f32, DeviceIndependentPixel>,
    ) -> bool {
        position.y < self.toolbar_height.get() || position.x < self.sidebar_width.get()
    }

    /// Create a frameless button with square sizing, as used in the toolbar.
    fn toolbar_button(text: &str) -> egui::Button<'_> {
        egui::Button::new(text)
            .frame(false)
            .min_size(Vec2 { x: 20.0, y: 20.0 })
    }

    /// One consistent 18px line-icon vocabulary, independent of font glyphs.
    fn icon(ui: &mut egui::Ui, name: &str, enabled: bool) -> egui::Response {
        let (rect, mut response) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::click());
        if !enabled { response = response.on_disabled_hover_text(name); }
        let color = if !enabled { ui.visuals().weak_text_color() }
            else { ui.visuals().text_color() };
        if enabled && response.hovered() {
            ui.painter().rect_filled(rect, 8, ui.visuals().widgets.hovered.weak_bg_fill);
        }
        let origin = rect.center() - egui::vec2(9.0, 9.0);
        let stroke = egui::Stroke::new(1.5, color);
        let paths: &[&[(f32, f32)]] = match name {
            "Back" => &[&[(11.,3.),(5.,9.),(11.,15.)]],
            "Forward" => &[&[(7.,3.),(13.,9.),(7.,15.)]],
            "Reload" => &[&[(14.,7.),(12.,3.),(7.,2.),(3.,5.),(2.,10.),(5.,14.),(10.,16.),(14.,13.)],&[(10.,7.),(15.,7.),(15.,2.)]],
            "New tab" => &[&[(3.,9.),(15.,9.)],&[(9.,3.),(9.,15.)]],
            "Close" => &[&[(5.,5.),(13.,13.)],&[(13.,5.),(5.,13.)]],
            "Home" => &[&[(2.,8.),(9.,2.),(16.,8.)],&[(4.,7.),(4.,16.),(14.,16.),(14.,7.)]],
            "Settings" => &[&[(2.,4.),(16.,4.)],&[(2.,9.),(16.,9.)],&[(2.,14.),(16.,14.)],&[(6.,2.),(6.,6.)],&[(12.,7.),(12.,11.)],&[(7.,12.),(7.,16.)]],
            "Collapse tab sidebar" => &[&[(11.,4.),(6.,9.),(11.,14.)]],
            "Expand tab sidebar" => &[&[(7.,4.),(12.,9.),(7.,14.)]],
            "New window" => &[&[(2.,3.),(16.,3.),(16.,15.),(2.,15.),(2.,3.)],&[(2.,7.),(16.,7.)]],
            _ => &[&[(4.,9.),(14.,9.)]],
        };
        for points in paths {
            for pair in points.windows(2) {
                ui.painter().line_segment([
                    origin + egui::vec2(pair[0].0,pair[0].1),
                    origin + egui::vec2(pair[1].0,pair[1].1),
                ],stroke);
            }
        }
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, name));
        response.on_hover_text(name)
    }

    fn browser_tab_vertical(
        ui: &mut egui::Ui, window: &ServoShellWindow, webview: WebView,
        favicon_texture: Option<egui::load::SizedTexture>, compact: bool,
        pinned: bool, pinned_tabs: &mut std::collections::HashSet<WebViewId>,
    ) {
        let label = webview.page_title().filter(|t| !t.is_empty())
            .or_else(|| webview.url().map(|u| u.to_string())).unwrap_or("New tab".into());
        let active = window.active_webview().map(|v|v.id()) == Some(webview.id());
        let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 36.0),egui::Sense::click());
        let fill = if active { ui.visuals().selection.bg_fill }
            else if response.hovered() { ui.visuals().widgets.hovered.weak_bg_fill }
            else { egui::Color32::TRANSPARENT };
        ui.painter().rect_filled(rect,8,fill);
        if active {
            ui.painter().rect_filled(egui::Rect::from_min_size(rect.min+egui::vec2(0.,8.),egui::vec2(3.,20.)),2,egui::Color32::from_rgb(171,208,177));
        }
        let center = rect.min+egui::vec2(22.,18.);
        if let Some(texture) = favicon_texture {
            ui.painter().image(texture.id,egui::Rect::from_center_size(center,egui::vec2(16.,16.)),
                egui::Rect::from_min_max(egui::pos2(0.,0.),egui::pos2(1.,1.)),egui::Color32::WHITE);
        } else {
            ui.painter().text(center,egui::Align2::CENTER_CENTER,truncate_with_ellipsis(&label,1),egui::FontId::proportional(14.),ui.visuals().text_color());
        }
        let compact = compact || pinned;
        let mut close_clicked=false;
        if !compact {
            let font=egui::FontId::proportional(13.);
            let text_color=if active {ui.visuals().text_color()} else {ui.visuals().weak_text_color()};
            let galley=ui.painter().layout(label.clone(),font,text_color,(rect.width()-78.).max(20.));
            // clip the title to a single line; full title remains in the tooltip.
            ui.painter().with_clip_rect(egui::Rect::from_min_size(rect.min+egui::vec2(40.,8.),egui::vec2((rect.width()-78.).max(20.),20.)))
                .galley(rect.min+egui::vec2(40.,9.),galley,text_color);
            let close=egui::Rect::from_center_size(egui::pos2(rect.right()-16.,rect.center().y),egui::vec2(24.,28.));
            let r=ui.interact(close,response.id.with("close"),egui::Sense::click());
            if active || response.hovered() || r.hovered() {
                let c=ui.visuals().weak_text_color(); let mid=close.center();
                ui.painter().line_segment([mid+egui::vec2(-3.,-3.),mid+egui::vec2(3.,3.)],egui::Stroke::new(1.5,c));
                ui.painter().line_segment([mid+egui::vec2(3.,-3.),mid+egui::vec2(-3.,3.)],egui::Stroke::new(1.5,c));
            }
            r.widget_info(||WidgetInfo::labeled(WidgetType::Button,true,"Close"));
            close_clicked=r.clicked();
        }
        response.widget_info(||WidgetInfo::labeled(WidgetType::Button,true,&label));
        if response.secondary_clicked() {
            if pinned {pinned_tabs.remove(&webview.id());} else {pinned_tabs.insert(webview.id());}
        } else if close_clicked || response.middle_clicked() {
            window.queue_user_interface_command(UserInterfaceCommand::CloseWebView(webview.id()));
        } else if response.clicked() && !active {window.activate_webview(webview.id());}
        response.on_hover_text(label);
        ui.add_space(4.);
    }

    /// Update the user interface, but do not paint the updated state.
    pub(crate) fn update(
        &mut self,
        state: &RunningAppState,
        window: &ServoShellWindow,
        headed_window: &headed_window::HeadedWindow,
    ) {
        self.rendering_context
            .make_current()
            .expect("Could not make RenderingContext current");
        let Self {
            rendering_context,
            context,
            toolbar_height,
            sidebar_width,
            sidebar_compact,
            pinned_tabs,
            location,
            location_dirty,
            favicon_textures,
            settings,
            settings_open,
            context_menu_pos,
            can_go_back,
            can_go_forward,
            ..
        } = self;

        let winit_window = headed_window.winit_window();
        context.run(winit_window, |ctx| {
            load_pending_favicons(ctx, window, favicon_textures);

            // Chromium-style browser keyboard shortcuts, handled at the window level so
            // they work regardless of which chrome widget has focus.
            let nav_modifiers = if cfg!(target_os = "macos") {
                Modifiers::COMMAND
            } else {
                Modifiers::CTRL
            };
            if ctx.input_mut(|i| i.consume_key(nav_modifiers, Key::T)) {
                window.queue_user_interface_command(UserInterfaceCommand::NewWebView);
            }
            if ctx.input_mut(|i| i.consume_key(nav_modifiers, Key::R)) {
                *location_dirty = false;
                window.queue_user_interface_command(UserInterfaceCommand::Reload);
            }
            if let Some(active) = window.active_webview() &&
                ctx.input_mut(|i| i.consume_key(nav_modifiers, Key::W))
            {
                window
                    .queue_user_interface_command(UserInterfaceCommand::CloseWebView(active.id()));
            }
            // Page screenshot: Ctrl+Shift+S saves the active page as a PNG under
            // %USERPROFILE%\Pictures\Argon (falling back to the current directory).
            if let Some(active) = window.active_webview() &&
                ctx.input_mut(|i| i.consume_key(nav_modifiers | Modifiers::SHIFT, Key::S))
            {
                let dir = std::env::var("USERPROFILE")
                    .map(|home| std::path::PathBuf::from(home).join("Pictures").join("Argon"))
                    .unwrap_or_else(|_| std::path::PathBuf::from("."));
                active.take_screenshot(None, move |image| {
                    match image {
                        Ok(image) => {
                            match fs::create_dir_all(&dir) {
                                Ok(()) => {
                                    let stamp = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|duration| duration.as_secs())
                                        .unwrap_or(0);
                                    let path = dir.join(format!("argon-{stamp}.png"));
                                    match image::DynamicImage::ImageRgba8(image).save(&path) {
                                        Ok(()) => info!("Saved page screenshot to {path:?}"),
                                        Err(error) =>
                                            warn!("Failed to save page screenshot: {error}."),
                                    }
                                },
                                Err(error) =>
                                    warn!("Could not create screenshot directory: {error}."),
                            }
                        },
                        Err(error) => warn!("Page screenshot capture failed: {error:?}."),
                    }
                });
            }
            // F5 reloads (Chromium parity; Ctrl+R is handled above).
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) {
                *location_dirty = false;
                window.queue_user_interface_command(UserInterfaceCommand::Reload);
            }
            // Escape reverts an in-progress address bar edit to the current URL.
            if *location_dirty && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                *location_dirty = false;
                if let Some(active) = window.active_webview() {
                    if let Some(url) = active.url() {
                        *location = url.to_string();
                    }
                }
            }
            // Tab switching: Ctrl+Tab / Ctrl+Shift+Tab cycle, Ctrl+1..=8 jump by index.
            let tab_count = window.webviews().len();
            if tab_count > 1 {
                if let Some(active_index) = window.get_active_webview_index() {
                    if ctx
                        .input_mut(|i| i.consume_key(nav_modifiers | Modifiers::SHIFT, Key::Tab))
                    {
                        window.activate_webview_by_index(
                            (active_index + tab_count - 1) % tab_count,
                        );
                    } else if ctx.input_mut(|i| i.consume_key(nav_modifiers, Key::Tab)) {
                        window.activate_webview_by_index((active_index + 1) % tab_count);
                    }
                }
                let number_keys = [
                    Key::Num1,
                    Key::Num2,
                    Key::Num3,
                    Key::Num4,
                    Key::Num5,
                    Key::Num6,
                    Key::Num7,
                    Key::Num8,
                ];
                for (index, key) in number_keys.into_iter().enumerate() {
                    if index < tab_count &&
                        ctx.input_mut(|i| i.consume_key(nav_modifiers, key))
                    {
                        window.activate_webview_by_index(index);
                    }
                }
            }

            // TODO: While in fullscreen add some way to mitigate the increased phishing risk
            // when not displaying the URL bar: https://github.com/servo/servo/issues/32443
            // Show toolbar unless fullscreen is from document (web API)
            if !headed_window.is_fullscreen_from_document() {
                // Original Argon workspace composition. Servo owns the content;
                // this layer owns navigation and never overlays false page chrome.
                let dark=settings.dark_theme;
                let surface=if dark {egui::Color32::from_rgba_unmultiplied(22,25,28,210)} else {egui::Color32::from_rgba_unmultiplied(244,247,245,220)};
                let rail=if dark {egui::Color32::from_rgba_unmultiplied(18,21,24,200)} else {egui::Color32::from_rgba_unmultiplied(235,240,237,214)};
                let accent=egui::Color32::from_rgb(171,208,177);
                ctx.style_mut(|style| {
                    style.spacing.item_spacing=egui::vec2(4.,8.);
                    style.visuals.window_fill=if dark {egui::Color32::from_rgba_unmultiplied(28,32,36,232)} else {egui::Color32::from_rgba_unmultiplied(245,248,246,238)};
                    style.visuals.window_stroke=egui::Stroke::new(1.,egui::Color32::from_white_alpha(if dark {28} else {60}));
                    style.visuals.window_corner_radius=16.into();
                    style.visuals.widgets.inactive.corner_radius=8.into();
                    style.visuals.widgets.hovered.corner_radius=8.into();
                    style.spacing.button_padding=egui::vec2(12.,8.);
                    style.visuals.selection.bg_fill=if dark {egui::Color32::from_rgb(41,50,44)} else {egui::Color32::from_rgb(207,226,210)};
                    style.visuals.selection.stroke.color=style.visuals.text_color();
                    style.visuals.widgets.hovered.weak_bg_fill=if dark {egui::Color32::from_rgb(39,41,44)} else {egui::Color32::from_rgb(218,222,219)};
                });
                let compact=*sidebar_compact;
                let rail_width=if compact {64.} else {248.};
                let toolbar_panel=Panel::top("argon_navigation")
                    .frame(egui::Frame::default().fill(surface).inner_margin(egui::Margin::symmetric(12,8)))
                    .show_inside(ctx,|ui| {
                        ui.set_height(40.);
                        ui.horizontal_centered(|ui| {
                            ui.allocate_ui(egui::vec2(rail_width-16.,36.),|ui| {
                                ui.horizontal_centered(|ui| {
                                    let (mark,_) = ui.allocate_exact_size(egui::vec2(24.,24.),egui::Sense::hover());
                                    let p=ui.painter(); let c=mark.center();
                                    p.line_segment([c+egui::vec2(-8.,7.),c+egui::vec2(0.,-8.)],egui::Stroke::new(2.,accent));
                                    p.line_segment([c+egui::vec2(0.,-8.),c+egui::vec2(8.,7.)],egui::Stroke::new(2.,accent));
                                    p.line_segment([c+egui::vec2(-4.,3.),c+egui::vec2(6.,3.)],egui::Stroke::new(2.,accent));
                                    if !compact {ui.label(egui::RichText::new("Argon").size(18.).strong());}
                                });
                            });
                            if Self::icon(ui,"Back",*can_go_back).clicked() && *can_go_back {
                                *location_dirty=false;window.queue_user_interface_command(UserInterfaceCommand::Back);
                            }
                            if Self::icon(ui,"Forward",*can_go_forward).clicked() && *can_go_forward {
                                *location_dirty=false;window.queue_user_interface_command(UserInterfaceCommand::Forward);
                            }
                            if Self::icon(ui,"Reload",true).clicked() {
                                *location_dirty=false;window.queue_user_interface_command(UserInterfaceCommand::Reload);
                            }
                            ui.add_space(8.);
                            let location_id=egui::Id::new("location_input");
                            let width=(ui.available_width()-88.).max(120.);
                            let field=ui.add_sized(egui::vec2(width,36.),egui::TextEdit::singleline(location)
                                .id(location_id).font(egui::FontId::proportional(14.))
                                .margin(egui::vec2(12.,8.)).hint_text("Search or enter address"));
                            if field.changed() {*location_dirty=true;}
                            if ui.input_mut(|i|i.consume_key(nav_modifiers,Key::L)||i.consume_key(Modifiers::ALT,Key::D)) {field.request_focus();}
                            if field.gained_focus() && let Some(mut state)=TextEditState::load(ui.ctx(),location_id) {
                                state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(0),CCursor::new(location.len()))));state.store(ui.ctx(),location_id);
                            }
                            if field.lost_focus() && ui.input(|i|i.key_pressed(Key::Enter)) {
                                window.queue_user_interface_command(UserInterfaceCommand::Go(location.clone()));
                            }
                            if Self::icon(ui,"Home",true).clicked() {
                                *location_dirty=false;window.queue_user_interface_command(UserInterfaceCommand::Go(settings.homepage.clone()));
                            }
                            if Self::icon(ui,"Settings",true).clicked() {*settings_open=!*settings_open;}
                        });
                    });
                *toolbar_height=Length::new(toolbar_panel.response.rect.max.y);
                let sidebar=Panel::left("argon_workspace_rail").resizable(false).exact_size(rail_width)
                    .frame(egui::Frame::default().fill(rail).inner_margin(egui::Margin::symmetric(12,12)))
                    .show_inside(ctx,|ui| {
                        if !compact {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Your tabs").size(12.).color(ui.visuals().weak_text_color()));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {
                                    ui.label(egui::RichText::new(window.webviews().len().to_string()).size(12.).color(ui.visuals().weak_text_color()));
                                });
                            });
                            ui.add_space(12.);
                        }
                        ui.horizontal(|ui| {
                            if Self::icon(ui,"New tab",true).clicked() {window.queue_user_interface_command(UserInterfaceCommand::NewWebView);}
                            if !compact && ui.add(egui::Button::new("New tab").frame(false)).clicked() {window.queue_user_interface_command(UserInterfaceCommand::NewWebView);}
                        });
                        ui.add_space(16.);
                        let list_height=(ui.available_height()-88.).max(36.);
                        egui::ScrollArea::vertical().max_height(list_height).show(ui,|ui| {
                            for (id,view) in window.webviews() {
                                if pinned_tabs.contains(&id) {
                                    let favicon=favicon_textures.get(&id).map(|(_,f)|f).copied();
                                    Self::browser_tab_vertical(ui,window,view,favicon,compact,true,pinned_tabs);
                                }
                            }
                            if !pinned_tabs.is_empty() {ui.separator();ui.add_space(8.);}
                            for (id,view) in window.webviews() {
                                if !pinned_tabs.contains(&id) {
                                    let favicon=favicon_textures.get(&id).map(|(_,f)|f).copied();
                                    Self::browser_tab_vertical(ui,window,view,favicon,compact,false,pinned_tabs);
                                }
                            }
                        });
                        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT),|ui| {
                            if !compact {ui.label(egui::RichText::new("Powered by Servo").size(11.).color(ui.visuals().weak_text_color()));ui.add_space(8.);}
                            ui.horizontal(|ui| {
                                let label=if compact {"Expand tab sidebar"} else {"Collapse tab sidebar"};
                                if Self::icon(ui,label,true).clicked() {
                                    *sidebar_compact=!*sidebar_compact;settings.sidebar_compact=*sidebar_compact;settings.save();
                                }
                                if !compact {
                                    if Self::icon(ui,"New window",true).clicked() {
                                        let url=Url::parse("servo:newtab").expect("internal new-tab URL");
                                        window.queue_user_interface_command(UserInterfaceCommand::NewWindow(TopLevelWebViewCreationRequest::WithUrl(url)));
                                    }
                                }
                            });
                        });
                    });
                *sidebar_width=Length::new(sidebar.response.rect.max.x);
            } else {
                *toolbar_height = Length::default();
                *sidebar_width = Length::default();
            }

            // A deliberate settings surface, not a loose stack of widgets.
            let mut open=*settings_open;
            egui::Window::new("Settings").open(&mut open).collapsible(false).resizable(false)
                .default_width(480.).show(ctx,|ui| {
                    ui.add_space(8.);
                    ui.label(egui::RichText::new("Make room for your web").size(24.));
                    ui.label(egui::RichText::new("Appearance and navigation").size(13.).color(ui.visuals().weak_text_color()));
                    ui.add_space(24.);
                    let mut changed=false;
                    ui.label(egui::RichText::new("Appearance").size(14.).strong());
                    ui.add_space(8.);
                    ui.horizontal(|ui| {
                        changed|=ui.radio_value(&mut settings.dark_theme,true,"Dark (Zen)").changed();
                        changed|=ui.radio_value(&mut settings.dark_theme,false,"Light").changed();
                    });
                    ui.add_space(12.);
                    changed|=ui.checkbox(&mut settings.sidebar_compact,"Compact tab sidebar").changed();
                    ui.label(egui::RichText::new("Keep more room for the page with an icon-width tab rail").size(12.).color(ui.visuals().weak_text_color()));
                    ui.add_space(24.);ui.separator();ui.add_space(16.);
                    ui.label(egui::RichText::new("Home address").size(14.).strong());
                    ui.add_space(8.);
                    changed|=ui.add_sized(egui::vec2(ui.available_width(),36.),egui::TextEdit::singleline(&mut settings.homepage).margin(egui::vec2(12.,8.))).changed();
                    ui.label(egui::RichText::new("Opened by Home in the navigation bar and page menu").size(12.).color(ui.visuals().weak_text_color()));
                    ui.add_space(24.);
                    let mut experimental=state.experimental_preferences_enabled();
                    if ui.checkbox(&mut experimental,"Experimental web features").changed() {
                        state.set_experimental_preferences_enabled(experimental);
                        *location_dirty=false;
                        window.queue_user_interface_command(UserInterfaceCommand::ReloadAll);
                    }
                    ui.add_space(16.);
                    if changed {
                        ctx.set_theme(if settings.dark_theme {egui::Theme::Dark} else {egui::Theme::Light});
                        *sidebar_compact=settings.sidebar_compact;settings.save();
                    }
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Saved on this device").size(12.).color(ui.visuals().weak_text_color()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{ui.hyperlink_to("Design credits","https://github.com/vio137/argon-windows/blob/main/NOTICES.md");});
                    });
                    ui.add_space(8.);
                });
            *settings_open=open;

            // Chrome context menu: opened by a secondary click over the page
            // (intercepted in headed_window), floating above the webview.
            if let Some(pos) = *context_menu_pos {
                let mut keep_open = true;
                let area = egui::Area::new(egui::Id::new("webview_context_menu"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(pos)
                    .show(ctx, |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.set_min_width(180.0);
                            if ui
                                .add_enabled(*can_go_back, egui::Button::new("Back"))
                                .clicked()
                            {
                                *location_dirty = false;
                                window.queue_user_interface_command(UserInterfaceCommand::Back);
                                keep_open = false;
                            }
                            if ui
                                .add_enabled(*can_go_forward, egui::Button::new("Forward"))
                                .clicked()
                            {
                                *location_dirty = false;
                                window.queue_user_interface_command(
                                    UserInterfaceCommand::Forward,
                                );
                                keep_open = false;
                            }
                            if ui.button("Reload").clicked() {
                                *location_dirty = false;
                                window.queue_user_interface_command(UserInterfaceCommand::Reload);
                                keep_open = false;
                            }
                            ui.separator();
                            if ui.button("Copy address").clicked() {
                                if let Some(active) = window.active_webview() &&
                                    let Some(url) = active.url()
                                {
                                    ctx.copy_text(url.to_string());
                                }
                                keep_open = false;
                            }
                            if ui.button("New tab").clicked() {
                                window.queue_user_interface_command(
                                    UserInterfaceCommand::NewWebView,
                                );
                                keep_open = false;
                            }
                            ui.separator();
                            if ui.button("Home").clicked() {
                                *location_dirty = false;
                                window.queue_user_interface_command(UserInterfaceCommand::Go(
                                    settings.homepage.clone(),
                                ));
                                keep_open = false;
                            }
                            if ui.button("Settings").clicked() {
                                *settings_open = true;
                                keep_open = false;
                            }
                        });
                    });
                // Clicking anywhere outside the popup dismisses it.
                if ctx.input(|i| i.pointer.any_pressed()) &&
                    let Some(pointer) = ctx.input(|i| i.pointer.interact_pos()) &&
                    !area.response.rect.contains(pointer)
                {
                    keep_open = false;
                }
                if !keep_open {
                    *context_menu_pos = None;
                }
            }

            let scale =
                Scale::<_, DeviceIndependentPixel, DevicePixel>::new(ctx.pixels_per_point());

            headed_window.for_each_active_dialog(window, |dialog| dialog.update(ctx));

            // If the top parts of the GUI changed size, then update the size of the WebView and also
            // the size of its RenderingContext.
            let available_rect = ctx.available_rect_before_wrap();

            // Build a graft node for each WebView.
            let affine = {
                // The grafted WebView tree reports bounds in device pixels relative to the
                // WebView's own origin, so this node supplies the offset of the WebView within
                // the window, and scales it to the same scale as the rest of the nodes in egui's
                // AccessKit tree.
                let scale = (1.0 / window.platform_window().hidpi_scale_factor().get()) as f64;
                let x = available_rect.min.x as f64;
                let y = available_rect.min.y as f64;
                Affine::new([scale, 0.0, 0.0, scale, x, y])
            };
            for (webview_id, webview) in window.webviews() {
                if let Some(tree_id) = webview.accesskit_tree_id() {
                    let id = egui::Id::new(webview_id);
                    ctx.accesskit_node_builder(id, |node| {
                        node.set_tree_id(tree_id);
                        // Only the transform is set: AccessKit consumers exclude graft nodes from
                        // the presented tree, so bounds on this node would never be read.
                        node.set_transform(affine);
                    });
                }
            }
            let size = Size2D::new(available_rect.width(), available_rect.height()) * scale;
            if let Some(webview) = window.active_webview() &&
                size != webview.size()
            {
                // `rect` is sized to just the WebView viewport, which is required by
                // `OffscreenRenderingContext` See:
                // <https://github.com/servo/servo/issues/38369#issuecomment-3138378527>
                webview.resize(PhysicalSize::new(size.width as u32, size.height as u32))
            }

            if let Some(status_text) = &self.status_text {
                egui::Tooltip::always_open(
                    ctx.clone(),
                    LayerId::new(Order::Tooltip, Id::new("tooltip")),
                    "tooltip layer".into(),
                    pos2(0.0, available_rect.max.y),
                )
                .show(|ui| ui.add(Label::new(status_text.clone()).extend()));
            }

            window.repaint_webviews();

            if let Some(render_to_parent) = rendering_context.render_to_parent_callback() {
                ctx.layer_painter(LayerId::background()).add(PaintCallback {
                    rect: available_rect,
                    callback: Arc::new(CallbackFn::new(move |info, painter| {
                        let clip = info.viewport_in_pixels();
                        let rect_in_parent = Rect::new(
                            Point2D::new(clip.left_px, clip.from_bottom_px),
                            Size2D::new(clip.width_px, clip.height_px),
                        );
                        render_to_parent(painter.gl(), rect_in_parent)
                    })),
                });
            }
        });

        // If any egui widget requested a repaint, also request a repaint for our
        // containing window. This allows egui widget to animate on their own.
        if self.context.egui_ctx.has_requested_repaint() {
            window.set_needs_repaint();
        }

        let adapter = self
            .context
            .egui_winit
            .accesskit
            .as_mut()
            .expect("guaranteed by Gui::new()");
        for tree_update in self.pending_accesskit_updates.drain(..) {
            adapter.update_if_active(|| tree_update);
        }
    }

    /// Paint the GUI, as of the last update.
    pub(crate) fn paint(&mut self, window: &Window) {
        self.rendering_context
            .make_current()
            .expect("Could not make RenderingContext current");
        self.rendering_context
            .parent_context()
            .prepare_for_rendering();
        self.context.paint(window);
        self.rendering_context.parent_context().present();
    }

    /// Updates the location field from the given [`RunningAppState`], unless the user has started
    /// editing it without clicking Go, returning true iff it has changed (needing an egui update).
    fn update_location_in_toolbar(&mut self, window: &ServoShellWindow) -> bool {
        // User edited without clicking Go?
        if self.location_dirty {
            return false;
        }

        let current_url_string = window
            .active_webview()
            .and_then(|webview| Some(webview.url()?.to_string()));
        match current_url_string {
            Some(location) if location != self.location => {
                self.location = location;
                true
            },
            _ => false,
        }
    }

    fn update_load_status(&mut self, window: &ServoShellWindow) -> bool {
        let state_status = window
            .active_webview()
            .map(|webview| webview.load_status())
            .unwrap_or(LoadStatus::Complete);
        let old_status = std::mem::replace(&mut self.load_status, state_status);
        let status_changed = old_status != self.load_status;

        // When the load status changes, we want the new changes to the URL to start
        // being reflected in the location bar.
        if status_changed {
            self.location_dirty = false;
        }

        status_changed
    }

    fn update_status_text(&mut self, window: &ServoShellWindow) -> bool {
        let state_status = window
            .active_webview()
            .and_then(|webview| webview.status_text());
        let old_status = std::mem::replace(&mut self.status_text, state_status);
        old_status != self.status_text
    }

    fn update_can_go_back_and_forward(&mut self, window: &ServoShellWindow) -> bool {
        let (can_go_back, can_go_forward) = window
            .active_webview()
            .map(|webview| (webview.can_go_back(), webview.can_go_forward()))
            .unwrap_or((false, false));
        let old_can_go_back = std::mem::replace(&mut self.can_go_back, can_go_back);
        let old_can_go_forward = std::mem::replace(&mut self.can_go_forward, can_go_forward);
        old_can_go_back != self.can_go_back || old_can_go_forward != self.can_go_forward
    }

    /// Updates all fields taken from the given [`ServoShellWindow`], such as the location field.
    /// Returns true iff the egui needs an update.
    pub(crate) fn update_webview_data(&mut self, window: &ServoShellWindow) -> bool {
        // Note: We must use the "bitwise OR" (|) operator here instead of "logical OR" (||)
        //       because logical OR would short-circuit if any of the functions return true.
        //       We want to ensure that all functions are called. The "bitwise OR" operator
        //       does not short-circuit.
        self.update_load_status(window) |
            self.update_location_in_toolbar(window) |
            self.update_status_text(window) |
            self.update_can_go_back_and_forward(window)
    }

    /// Returns true if a redraw is required after handling the provided event.
    pub(crate) fn handle_accesskit_event(
        &mut self,
        event: &egui_winit::accesskit_winit::WindowEvent,
    ) -> bool {
        match event {
            egui_winit::accesskit_winit::WindowEvent::InitialTreeRequested => {
                self.context.egui_ctx.enable_accesskit();
                true
            },
            egui_winit::accesskit_winit::WindowEvent::ActionRequested(req) => {
                self.context
                    .egui_winit
                    .on_accesskit_action_request(req.clone());
                true
            },
            egui_winit::accesskit_winit::WindowEvent::AccessibilityDeactivated => {
                self.context.egui_ctx.disable_accesskit();
                false
            },
        }
    }

    pub(crate) fn set_zoom_factor(&self, factor: f32) {
        self.context.egui_ctx.set_zoom_factor(factor);
    }

    pub(crate) fn notify_accessibility_tree_update(&mut self, tree_update: accesskit::TreeUpdate) {
        self.pending_accesskit_updates.push(tree_update);
    }
}

fn embedder_image_to_egui_image(image: &Image) -> egui::ColorImage {
    let width = image.width as usize;
    let height = image.height as usize;

    match image.format {
        PixelFormat::K8 => egui::ColorImage::from_gray([width, height], image.data()),
        PixelFormat::KA8 => {
            // Convert to rgba
            let data: Vec<u8> = image
                .data()
                .chunks_exact(2)
                .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
                .collect();
            egui::ColorImage::from_rgba_unmultiplied([width, height], &data)
        },
        PixelFormat::RGB8 => egui::ColorImage::from_rgb([width, height], image.data()),
        PixelFormat::RGBA8 => {
            egui::ColorImage::from_rgba_unmultiplied([width, height], image.data())
        },
        PixelFormat::BGRA8 => {
            // Convert from BGRA to RGBA
            let data: Vec<u8> = image
                .data()
                .chunks_exact(4)
                .flat_map(|chunk| [chunk[2], chunk[1], chunk[0], chunk[3]])
                .collect();
            egui::ColorImage::from_rgba_unmultiplied([width, height], &data)
        },
    }
}

/// Uploads all favicons that have not yet been processed to the GPU.
fn load_pending_favicons(
    ctx: &egui::Context,
    window: &ServoShellWindow,
    texture_cache: &mut HashMap<WebViewId, (egui::TextureHandle, egui::load::SizedTexture)>,
) {
    for id in window.take_pending_favicon_loads() {
        let Some(webview) = window.webview_by_id(id) else {
            continue;
        };
        let Some(favicon) = webview.favicon() else {
            continue;
        };

        let egui_image = embedder_image_to_egui_image(&favicon);
        let handle = ctx.load_texture(format!("favicon-{id:?}"), egui_image, Default::default());
        let texture = egui::load::SizedTexture::new(
            handle.id(),
            egui::vec2(favicon.width as f32, favicon.height as f32),
        );

        // We don't need the handle anymore but we can't drop it either since that would cause
        // the texture to be freed.
        texture_cache.insert(id, (handle, texture));
    }
}
