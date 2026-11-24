//! The app's look: colors, type, sizes, and the egui styles built from them.
//! `DESIGN.md` explains the choices. Every color the app paints comes from here, so a change in
//! one place restyles the whole window.

use std::sync::Arc;

use aic_editor::style::{self, fonts};
use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, RichText,
    Shadow, Stroke, Style, TextStyle, Vec2, Visuals,
};

/// The palette. Neutrals step up from the canvas (darkest) to panels, then cards and popovers.
/// Hues carry meaning only: the accent is selection and focus, the rest are states.
pub mod color {
    use super::rgba;
    use aic_editor::style;
    use eframe::egui::Color32;

    /// The node canvas, the darkest surface.
    pub const CANVAS: Color32 = rgba(style::color::CANVAS);
    /// Side panels, menu bar, toolbar, status bar.
    pub const PANEL: Color32 = rgba(style::color::PANEL);
    /// Node cards, popovers, windows, menus.
    pub const RAISED: Color32 = rgba(style::color::RAISED);
    /// Text fields and other inset wells.
    pub const INSET: Color32 = rgba(style::color::INSET);
    /// Every other row of a striped list.
    pub const STRIPE: Color32 = rgba(style::color::STRIPE);

    /// A button at rest.
    pub const CONTROL: Color32 = rgba(style::color::CONTROL);
    /// A button under the pointer, or an open menu's button.
    pub const CONTROL_HOVER: Color32 = rgba(style::color::CONTROL_HOVER);
    /// A button being pressed.
    pub const CONTROL_ACTIVE: Color32 = rgba(style::color::CONTROL_ACTIVE);

    /// Dividers between panels, and the canvas grid.
    pub const DIVIDER: Color32 = rgba(style::color::DIVIDER);
    /// Outlines of cards, buttons and fields.
    pub const BORDER: Color32 = rgba(style::color::BORDER);
    /// Outlines under the pointer.
    pub const BORDER_STRONG: Color32 = rgba(style::color::BORDER_STRONG);
    /// The canvas grid: a step above the canvas, so it is there without being read.
    pub const GRID: Color32 = rgba(style::color::GRID);

    /// Headings, hovered text, numbers that matter.
    pub const TEXT_STRONG: Color32 = rgba(style::color::TEXT_STRONG);
    /// Labels and body text.
    pub const TEXT: Color32 = rgba(style::color::TEXT);
    /// Secondary text: units, facility names, captions.
    pub const TEXT_MUTED: Color32 = rgba(style::color::TEXT_MUTED);
    /// Hints and placeholders. Still 4.5:1 on panels.
    pub const TEXT_FAINT: Color32 = rgba(style::color::TEXT_FAINT);

    /// Selection and keyboard focus. Nothing else is blue.
    pub const ACCENT: Color32 = rgba(style::color::ACCENT);
    /// Behind selected text, selected rows and toggled buttons.
    pub const ACCENT_FILL: Color32 = rgba(style::color::ACCENT_FILL);
    /// Text on [`ACCENT_FILL`], and the outline of a focused field (egui draws both with one
    /// stroke).
    pub const ACCENT_TEXT: Color32 = rgba(style::color::ACCENT_TEXT);

    /// An input port pulling from the depot (and a source node, which does the same).
    pub const DEPOT_IN: Color32 = rgba(style::color::DEPOT_IN);
    /// An output port sending to the depot (and a sink node, which does the same).
    pub const DEPOT_OUT: Color32 = rgba(style::color::DEPOT_OUT);
    /// A linked port, and links.
    pub const LINKED: Color32 = rgba(style::color::LINKED);
    /// Throttle markers and hints such as a link over one belt.
    pub const WARN: Color32 = rgba(style::color::WARN);
    /// A storage filling up.
    pub const POSITIVE: Color32 = rgba(style::color::POSITIVE);
    /// A storage draining, text that doesn't parse, errors.
    pub const NEGATIVE: Color32 = rgba(style::color::NEGATIVE);
    /// The box a right-drag draws to select.
    pub const SELECT_BOX: Color32 = ACCENT;

    /// Node card headers: a low-chroma tint per node kind, so kinds tell apart at a glance while
    /// the header text stays the loudest thing on it.
    pub mod node {
        use super::rgba;
        use aic_editor::style;
        use eframe::egui::Color32;

        pub const RECIPE: Color32 = rgba(style::color::node::RECIPE);
        /// Pink family, like a port pulling from the depot.
        pub const SOURCE: Color32 = rgba(style::color::node::SOURCE);
        /// Green family, like a port sending to the depot.
        pub const SINK: Color32 = rgba(style::color::node::SINK);
        pub const OUTPOST_ORDER: Color32 = rgba(style::color::node::OUTPOST_ORDER);
        pub const MISSING: Color32 = rgba(style::color::node::MISSING);
        /// Splitters, mergers, storage, control ports and workspace ports.
        pub const LOGISTICS: Color32 = rgba(style::color::node::LOGISTICS);
        pub const WORKSPACE: Color32 = rgba(style::color::node::WORKSPACE);
    }

    /// Region badges: one saturated tint per region, in the data's region order (Valley IV, then
    /// Wuling), so a region reads the same everywhere and two regions tell apart without reading.
    /// Yellow and teal; the yellow is a cooler lemon than the warning's amber.
    pub mod region {
        use super::rgba;
        use aic_editor::style;
        use eframe::egui::Color32;

        pub struct Tint {
            pub fill: Color32,
            pub stroke: Color32,
            pub text: Color32,
        }

        pub const TINTS: [Tint; 2] = [
            tint(style::color::region::TINTS[0]),
            tint(style::color::region::TINTS[1]),
        ];

        const fn tint(tint: style::color::region::Tint) -> Tint {
            Tint {
                fill: rgba(tint.fill),
                stroke: rgba(tint.stroke),
                text: rgba(tint.text),
            }
        }
    }
}

/// A design token as egui's color.
const fn rgba(c: style::Rgba) -> Color32 {
    Color32::from_rgba_premultiplied(c.0, c.1, c.2, c.3)
}

pub use aic_editor::style::{radius, text};

/// A design shadow as egui's.
const fn shadow(s: style::Shadow) -> Shadow {
    Shadow {
        offset: s.offset,
        blur: s.blur,
        spread: 0,
        color: Color32::from_black_alpha(s.alpha),
    }
}

/// Font family of the semibold UI face, for headings and emphasis.
pub fn semibold() -> FontFamily {
    FontFamily::Name(SEMIBOLD.into())
}

/// Font family of the semibold tabular face, for the number a card is about.
pub fn tabular_semibold() -> FontFamily {
    FontFamily::Name(TABULAR_SEMIBOLD.into())
}

/// The number a card is about, at `size`: semibold, tabular.
pub fn number(size: f32) -> FontId {
    FontId::new(size, tabular_semibold())
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
        .inner_margin(Margin::same(style::spacing::CARD_MARGIN))
        .shadow(shadow(style::shadow::CARD))
}

const SEMIBOLD: &str = "semibold";
const TABULAR_SEMIBOLD: &str = "tabular-semibold";
const NUMBER_STYLE: &str = "number";

/// Inter Display and its tabular cuts in front of egui's default fonts. egui's own faces stay
/// behind them as fallbacks for the symbols and emoji the toolbar draws. egui's `Monospace` family
/// is the tabular face, so `.monospace()` sets a number.
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
    let regular = family(
        "inter",
        FontData::from_static(fonts::INTER_LIGHT),
        &proportional,
    );
    let regular_bold = family(
        "inter-semibold",
        FontData::from_static(fonts::INTER_SEMIBOLD),
        &regular,
    );
    let mono = family(
        "inter-tabular",
        FontData::from_static(fonts::INTER_TABULAR_LIGHT),
        &monospace,
    );
    let mono_bold = family(
        "inter-tabular-semibold",
        FontData::from_static(fonts::INTER_TABULAR_SEMIBOLD),
        &mono,
    );

    fonts.families.insert(FontFamily::Proportional, regular);
    fonts.families.insert(FontFamily::Monospace, mono);
    fonts.families.insert(semibold(), regular_bold);
    fonts.families.insert(tabular_semibold(), mono_bold);
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
    spacing.item_spacing = Vec2::from(style::spacing::ITEM);
    spacing.button_padding = Vec2::from(style::spacing::BUTTON_PADDING);
    spacing.interact_size = Vec2::from(style::spacing::INTERACT_SIZE);
    spacing.window_margin = Margin::same(style::spacing::WINDOW_MARGIN);
    spacing.menu_margin = Margin::same(style::spacing::MENU_MARGIN);
    spacing.indent = style::spacing::INDENT;
    spacing.icon_width = style::spacing::ICON_WIDTH;
    spacing.icon_width_inner = style::spacing::ICON_WIDTH_INNER;
    spacing.icon_spacing = style::spacing::ICON_SPACING;
    spacing.combo_height = style::spacing::COMBO_HEIGHT;
    spacing.scroll = egui::style::ScrollStyle {
        foreground_color: true,
        dormant_handle_opacity: style::scroll::HANDLE,
        active_handle_opacity: style::scroll::HANDLE_NEAR,
        interact_handle_opacity: style::scroll::HANDLE_HELD,
        ..egui::style::ScrollStyle::thin()
    };

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
    v.window_shadow = shadow(style::shadow::WINDOW);
    v.window_highlight_topmost = false;
    v.menu_corner_radius = CornerRadius::same(radius::MEDIUM);
    v.popup_shadow = shadow(style::shadow::POPOVER);

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

    #[test]
    fn every_family_has_a_font() {
        let fonts = fonts();
        for family in [
            FontFamily::Proportional,
            FontFamily::Monospace,
            semibold(),
            tabular_semibold(),
        ] {
            let list = &fonts.families[&family];
            assert!(!list.is_empty(), "{family:?}");
            assert!(list.iter().all(|name| fonts.font_data.contains_key(name)));
        }
    }

    #[test]
    fn the_tabular_faces_set_every_digit_one_width() {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts());
        // Fonts load on the first pass; no renderer takes its textures.
        ctx.run_ui(Default::default(), |_| {})
            .textures_delta
            .clear();
        let width = |text: &str, family: FontFamily| {
            ctx.fonts_mut(|fonts| {
                let font = FontId::new(text::MONO, family);
                fonts
                    .layout_no_wrap(text.to_owned(), font, Color32::WHITE)
                    .size()
                    .x
            })
        };
        // Inter's own digits differ, which is why the tabular cuts exist.
        assert!(width("1111", FontFamily::Proportional) < width("0000", FontFamily::Proportional));
        for family in [FontFamily::Monospace, tabular_semibold()] {
            let zero = width("0000", family.clone());
            for digit in '1'..='9' {
                let digits: String = std::iter::repeat_n(digit, 4).collect();
                assert_eq!(
                    width(&digits, family.clone()),
                    zero,
                    "{digit} in {family:?}"
                );
            }
        }
    }
}
