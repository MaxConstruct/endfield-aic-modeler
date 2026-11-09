//! The app's look: colors, type, sizes, and the egui and egui-snarl styles built from them.
//! `DESIGN.md` explains the choices. Every color the app paints comes from here, so a change in
//! one place restyles the whole window.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, RichText,
    Shadow, Stroke, Style, TextStyle, Vec2, Visuals, epaint::text::VariationCoords,
};

/// The palette. Neutrals step up from the canvas (darkest) to panels, then cards and popovers.
/// Hues carry meaning only: the accent is selection and focus, the rest are states.
pub mod color {
    use eframe::egui::Color32;

    /// The node canvas, the darkest surface.
    pub const CANVAS: Color32 = Color32::from_rgb(0x11, 0x12, 0x14);
    /// Side panels, menu bar, toolbar, status bar.
    pub const PANEL: Color32 = Color32::from_rgb(0x18, 0x19, 0x1C);
    /// Node cards, popovers, windows, menus.
    pub const RAISED: Color32 = Color32::from_rgb(0x1F, 0x20, 0x24);
    /// Text fields and other inset wells.
    pub const INSET: Color32 = Color32::from_rgb(0x0F, 0x10, 0x12);
    /// Every other row of a striped list.
    pub const STRIPE: Color32 = Color32::from_rgb(0x1B, 0x1C, 0x20);

    /// A button at rest.
    pub const CONTROL: Color32 = Color32::from_rgb(0x26, 0x28, 0x2D);
    /// A button under the pointer, or an open menu's button.
    pub const CONTROL_HOVER: Color32 = Color32::from_rgb(0x2E, 0x30, 0x36);
    /// A button being pressed.
    pub const CONTROL_ACTIVE: Color32 = Color32::from_rgb(0x36, 0x38, 0x3F);

    /// Dividers between panels, and the canvas grid.
    pub const DIVIDER: Color32 = Color32::from_rgb(0x26, 0x27, 0x2B);
    /// Outlines of cards, buttons and fields.
    pub const BORDER: Color32 = Color32::from_rgb(0x30, 0x32, 0x38);
    /// Outlines under the pointer.
    pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x44, 0x47, 0x4F);
    /// The canvas grid: a step above the canvas, so it is there without being read.
    pub const GRID: Color32 = Color32::from_rgb(0x1C, 0x1D, 0x21);

    /// Headings, hovered text, numbers that matter.
    pub const TEXT_STRONG: Color32 = Color32::from_rgb(0xF3, 0xF4, 0xF6);
    /// Labels and body text.
    pub const TEXT: Color32 = Color32::from_rgb(0xD3, 0xD5, 0xDA);
    /// Secondary text: units, facility names, captions.
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(0xA0, 0xA3, 0xAB);
    /// Hints and placeholders. Still 4.5:1 on panels.
    pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x86, 0x89, 0x91);

    /// Selection and keyboard focus. Nothing else is blue.
    pub const ACCENT: Color32 = Color32::from_rgb(0x6E, 0x9B, 0xFF);
    /// Behind selected text, selected rows and toggled buttons.
    pub const ACCENT_FILL: Color32 = Color32::from_rgb(0x24, 0x37, 0x61);
    /// Text on [`ACCENT_FILL`], and the outline of a focused field (egui draws both with one
    /// stroke).
    pub const ACCENT_TEXT: Color32 = Color32::from_rgb(0x9C, 0xBB, 0xFF);

    /// An input port pulling from the depot (and a source node, which does the same).
    pub const DEPOT_IN: Color32 = Color32::from_rgb(0xF0, 0x7A, 0xAE);
    /// An output port sending to the depot (and a sink node, which does the same).
    pub const DEPOT_OUT: Color32 = Color32::from_rgb(0x6C, 0xC7, 0x7E);
    /// A linked port, and links.
    pub const LINKED: Color32 = Color32::from_rgb(0x9A, 0xA0, 0xAB);
    /// Throttle markers and hints such as a link over one belt.
    pub const WARN: Color32 = Color32::from_rgb(0xE9, 0xA2, 0x3B);
    /// A storage filling up.
    pub const POSITIVE: Color32 = Color32::from_rgb(0x6C, 0xC7, 0x7E);
    /// A storage draining, text that doesn't parse, errors.
    pub const NEGATIVE: Color32 = Color32::from_rgb(0xEF, 0x64, 0x61);
    /// The box a right-drag draws to select.
    pub const SELECT_BOX: Color32 = ACCENT;

    /// Node card headers: a low-chroma tint per node kind, so kinds tell apart at a glance while
    /// the header text stays the loudest thing on it.
    pub mod node {
        use eframe::egui::Color32;

        pub const RECIPE: Color32 = Color32::from_rgb(0x28, 0x33, 0x49);
        /// Pink family, like a port pulling from the depot.
        pub const SOURCE: Color32 = Color32::from_rgb(0x4A, 0x2A, 0x3C);
        /// Green family, like a port sending to the depot.
        pub const SINK: Color32 = Color32::from_rgb(0x21, 0x3A, 0x29);
        pub const OUTPOST_ORDER: Color32 = Color32::from_rgb(0x45, 0x36, 0x1C);
        pub const MISSING: Color32 = Color32::from_rgb(0x52, 0x24, 0x26);
        /// Splitters, mergers, storage, control ports and workspace ports.
        pub const LOGISTICS: Color32 = Color32::from_rgb(0x2E, 0x30, 0x36);
        pub const WORKSPACE: Color32 = Color32::from_rgb(0x3A, 0x30, 0x52);
    }
}

/// Corner radii, in points.
pub mod radius {
    /// Buttons, fields, chips, link labels.
    pub const SMALL: u8 = 4;
    /// Node cards, menus, popovers.
    pub const MEDIUM: u8 = 6;
    /// Windows.
    pub const LARGE: u8 = 8;
}

/// Type sizes, in points. At 1x a point is a pixel; the steps are about 1.15 apart.
pub mod text {
    /// Link labels, captions, the status bar.
    pub const SMALL: f32 = 12.0;
    /// Labels, buttons, list rows.
    pub const BODY: f32 = 13.0;
    /// Numbers in panels, port rates.
    pub const MONO: f32 = 13.0;
    /// Panel titles.
    pub const HEADING: f32 = 15.0;
    /// The number a card is about: its facility count, a source's supply.
    pub const NUMBER: f32 = 18.0;
}

/// Font family of the semibold UI face, for headings and emphasis.
pub fn semibold() -> FontFamily {
    FontFamily::Name(SEMIBOLD.into())
}

/// Font family of the semibold mono face, for the number a card is about.
pub fn mono_semibold() -> FontFamily {
    FontFamily::Name(MONO_SEMIBOLD.into())
}

/// The number a card is about, at `size`: semibold, tabular.
pub fn number(size: f32) -> FontId {
    FontId::new(size, mono_semibold())
}

/// Emphasized text: the semibold face in the strongest text color. egui's own `strong()` only
/// changes the color.
pub fn strong(text: impl Into<String>) -> RichText {
    RichText::new(text)
        .family(semibold())
        .color(color::TEXT_STRONG)
}

/// The text style for the number a card is about.
pub fn number_style() -> TextStyle {
    TextStyle::Name(NUMBER_STYLE.into())
}

/// Sets the app's fonts and style on `ctx`. Call once, before the first frame.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.style_mut_of(egui::Theme::Dark, apply);
}

/// The style of a node card: a raised surface with a hairline outline and a short shadow.
pub fn node_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(color::RAISED)
        .stroke(Stroke::new(1.0, color::BORDER))
        .corner_radius(CornerRadius::same(radius::MEDIUM))
        .inner_margin(Margin::same(6))
        .shadow(Shadow {
            offset: [0, 2],
            blur: 6,
            spread: 0,
            color: Color32::from_black_alpha(90),
        })
}

const SEMIBOLD: &str = "semibold";
const MONO_SEMIBOLD: &str = "mono-semibold";
const NUMBER_STYLE: &str = "number";

/// The app's faces, embedded so it looks the same on macOS and Windows. Both are under the SIL Open
/// Font License (`fonts/*-LICENSE.txt`). Inter carries words in static Regular and SemiBold cuts.
/// JetBrains Mono carries numbers that must line up (egui can't turn on Inter's tabular figures);
/// it is one variable file, set to each weight.
const INTER_REGULAR: &[u8] = include_bytes!("../fonts/Inter-Regular.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../fonts/Inter-SemiBold.ttf");
const JETBRAINS_MONO: &[u8] = include_bytes!("../fonts/JetBrainsMono-Variable.ttf");

/// JetBrains Mono at `weight`.
fn mono_at(weight: f32) -> FontData {
    let mut data = FontData::from_static(JETBRAINS_MONO);
    data.tweak.coords = VariationCoords::new([(b"wght", weight)]);
    data
}

/// Inter and JetBrains Mono in front of egui's default fonts. egui's own faces stay behind them as
/// fallbacks for the symbols and emoji the toolbar draws.
fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let defaults = |family: &FontFamily, fonts: &FontDefinitions| {
        fonts.families.get(family).cloned().unwrap_or_default()
    };
    let proportional = defaults(&FontFamily::Proportional, &fonts);
    let monospace = defaults(&FontFamily::Monospace, &fonts);

    let mut family = |name: &str, data: FontData, fallback: &[String]| -> Vec<String> {
        fonts.font_data.insert(name.to_owned(), Arc::new(data));
        let mut list = vec![name.to_owned()];
        list.extend(fallback.iter().cloned());
        list
    };
    let regular = family("inter", FontData::from_static(INTER_REGULAR), &proportional);
    let regular_bold = family(
        "inter-semibold",
        FontData::from_static(INTER_SEMIBOLD),
        &regular,
    );
    let mono = family("jetbrains-mono", mono_at(400.0), &monospace);
    let mono_bold = family("jetbrains-mono-semibold", mono_at(600.0), &mono);

    fonts.families.insert(FontFamily::Proportional, regular);
    fonts.families.insert(FontFamily::Monospace, mono);
    fonts.families.insert(semibold(), regular_bold);
    fonts.families.insert(mono_semibold(), mono_bold);
    fonts
}

/// The app's style over egui's dark style.
fn apply(style: &mut Style) {
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(text::SMALL)),
        (TextStyle::Body, FontId::proportional(text::BODY)),
        (TextStyle::Button, FontId::proportional(text::BODY)),
        (TextStyle::Monospace, FontId::monospace(text::MONO)),
        (TextStyle::Heading, FontId::new(text::HEADING, semibold())),
        (number_style(), number(text::NUMBER)),
    ]
    .into();

    let spacing = &mut style.spacing;
    spacing.item_spacing = Vec2::new(8.0, 3.0);
    spacing.button_padding = Vec2::new(8.0, 2.0);
    spacing.interact_size = Vec2::new(40.0, 20.0);
    spacing.window_margin = Margin::same(10);
    spacing.menu_margin = Margin::same(4);
    spacing.indent = 16.0;
    spacing.icon_width = 14.0;
    spacing.icon_width_inner = 8.0;
    spacing.icon_spacing = 6.0;
    spacing.combo_height = 320.0;
    spacing.scroll = egui::style::ScrollStyle::thin();

    style.visuals = visuals();
}

fn visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.text_options.subpixel_binning = false;
    v.weak_text_color = Some(color::TEXT_FAINT);
    v.hyperlink_color = color::TEXT_MUTED;
    v.faint_bg_color = color::STRIPE;
    v.extreme_bg_color = color::INSET;
    v.text_edit_bg_color = Some(color::INSET);
    v.code_bg_color = color::CONTROL;
    v.warn_fg_color = color::WARN;
    v.error_fg_color = color::NEGATIVE;

    v.panel_fill = color::PANEL;
    v.window_fill = color::RAISED;
    v.window_stroke = Stroke::new(1.0, color::BORDER);
    v.window_corner_radius = CornerRadius::same(radius::LARGE);
    v.window_shadow = Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(140),
    };
    v.window_highlight_topmost = false;
    v.menu_corner_radius = CornerRadius::same(radius::MEDIUM);
    v.popup_shadow = Shadow {
        offset: [0, 4],
        blur: 14,
        spread: 0,
        color: Color32::from_black_alpha(120),
    };

    v.selection.bg_fill = color::ACCENT_FILL;
    v.selection.stroke = Stroke::new(1.0, color::ACCENT_TEXT);
    v.text_cursor.stroke = Stroke::new(2.0, color::ACCENT);
    v.text_cursor.blink = false;

    let control = |bg: Color32, border: Color32, text: Color32| egui::style::WidgetVisuals {
        bg_fill: bg,
        weak_bg_fill: bg,
        bg_stroke: Stroke::new(1.0, border),
        corner_radius: CornerRadius::same(radius::SMALL),
        fg_stroke: Stroke::new(1.0, text),
        expansion: 0.0,
    };
    // Labels, separators and frames that don't respond.
    v.widgets.noninteractive = control(color::PANEL, color::DIVIDER, color::TEXT);
    v.widgets.inactive = control(color::CONTROL, color::DIVIDER, color::TEXT);
    v.widgets.hovered = control(
        color::CONTROL_HOVER,
        color::BORDER_STRONG,
        color::TEXT_STRONG,
    );
    v.widgets.active = control(color::CONTROL_ACTIVE, color::ACCENT, color::TEXT_STRONG);
    v.widgets.open = control(
        color::CONTROL_HOVER,
        color::BORDER_STRONG,
        color::TEXT_STRONG,
    );
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Relative luminance (WCAG 2), for contrast checks.
    fn luminance(c: Color32) -> f32 {
        let channel = |v: u8| {
            let v = f32::from(v) / 255.0;
            if v <= 0.039_28 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
    }

    fn contrast(a: Color32, b: Color32) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn text_reads_on_every_surface() {
        for surface in [color::CANVAS, color::PANEL, color::RAISED, color::INSET] {
            for text in [
                color::TEXT_STRONG,
                color::TEXT,
                color::TEXT_MUTED,
                color::TEXT_FAINT,
            ] {
                assert!(contrast(text, surface) >= 4.5, "{text:?} on {surface:?}");
            }
            for state in [
                color::ACCENT,
                color::DEPOT_IN,
                color::DEPOT_OUT,
                color::WARN,
                color::NEGATIVE,
                color::LINKED,
            ] {
                assert!(contrast(state, surface) >= 4.5, "{state:?} on {surface:?}");
            }
        }
    }

    #[test]
    fn header_text_reads_on_every_node_tint() {
        use color::node::*;
        for tint in [
            RECIPE,
            SOURCE,
            SINK,
            OUTPOST_ORDER,
            MISSING,
            LOGISTICS,
            WORKSPACE,
        ] {
            assert!(contrast(color::TEXT_STRONG, tint) >= 7.0, "{tint:?}");
            assert!(contrast(color::TEXT_MUTED, tint) >= 4.5, "{tint:?}");
        }
        assert!(contrast(color::ACCENT_TEXT, color::ACCENT_FILL) >= 4.5);
    }

    #[test]
    fn every_family_has_a_font() {
        let fonts = fonts();
        for family in [
            FontFamily::Proportional,
            FontFamily::Monospace,
            semibold(),
            mono_semibold(),
        ] {
            let list = &fonts.families[&family];
            assert!(!list.is_empty(), "{family:?}");
            assert!(list.iter().all(|name| fonts.font_data.contains_key(name)));
        }
    }
}
