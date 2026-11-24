//! The design tokens of `DESIGN.md` as plain data: colors, radii, type sizes, spacing, shadows and
//! the fonts. No toolkit here; the app converts these to egui's types. This file is the source of
//! truth for the look (`DESIGN.md` explains the choices).

/// An sRGB color, red, green, blue, alpha, each 0 to 255.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

impl Rgba {
    /// An opaque color.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self(r, g, b, 255)
    }

    /// Black with the given alpha, as shadows use.
    pub const fn black_alpha(alpha: u8) -> Self {
        Self(0, 0, 0, alpha)
    }
}

/// The palette. Neutrals step up from the canvas (darkest) to panels, then cards and popovers.
/// Hues carry meaning only: the accent is selection and focus, the rest are states.
pub mod color {
    use super::Rgba;

    /// The node canvas, the darkest surface.
    pub const CANVAS: Rgba = Rgba::rgb(0x11, 0x12, 0x14);
    /// Side panels, menu bar, toolbar, status bar.
    pub const PANEL: Rgba = Rgba::rgb(0x18, 0x19, 0x1C);
    /// Node cards, popovers, windows, menus.
    pub const RAISED: Rgba = Rgba::rgb(0x1F, 0x20, 0x24);
    /// Text fields and other inset wells.
    pub const INSET: Rgba = Rgba::rgb(0x0F, 0x10, 0x12);
    /// Every other row of a striped list.
    pub const STRIPE: Rgba = Rgba::rgb(0x1B, 0x1C, 0x20);

    /// A button at rest.
    pub const CONTROL: Rgba = Rgba::rgb(0x26, 0x28, 0x2D);
    /// A button under the pointer, or an open menu's button.
    pub const CONTROL_HOVER: Rgba = Rgba::rgb(0x2E, 0x30, 0x36);
    /// A button being pressed.
    pub const CONTROL_ACTIVE: Rgba = Rgba::rgb(0x36, 0x38, 0x3F);

    /// Dividers between panels, and the canvas grid.
    pub const DIVIDER: Rgba = Rgba::rgb(0x26, 0x27, 0x2B);
    /// Outlines of cards, buttons and fields.
    pub const BORDER: Rgba = Rgba::rgb(0x30, 0x32, 0x38);
    /// Outlines under the pointer.
    pub const BORDER_STRONG: Rgba = Rgba::rgb(0x44, 0x47, 0x4F);
    /// The canvas grid: a step above the canvas, so it is there without being read.
    pub const GRID: Rgba = Rgba::rgb(0x1C, 0x1D, 0x21);

    /// Headings, hovered text, numbers that matter.
    pub const TEXT_STRONG: Rgba = Rgba::rgb(0xF3, 0xF4, 0xF6);
    /// Labels and body text.
    pub const TEXT: Rgba = Rgba::rgb(0xD3, 0xD5, 0xDA);
    /// Secondary text: units, facility names, captions.
    pub const TEXT_MUTED: Rgba = Rgba::rgb(0xA0, 0xA3, 0xAB);
    /// Hints and placeholders. Still 4.5:1 on panels.
    pub const TEXT_FAINT: Rgba = Rgba::rgb(0x86, 0x89, 0x91);

    /// Selection and keyboard focus. Nothing else is blue.
    pub const ACCENT: Rgba = Rgba::rgb(0x6E, 0x9B, 0xFF);
    /// Behind selected text, selected rows and toggled buttons.
    pub const ACCENT_FILL: Rgba = Rgba::rgb(0x24, 0x37, 0x61);
    /// Text on [`ACCENT_FILL`], and the outline of a focused field (a toolkit may draw both with one
    /// stroke).
    pub const ACCENT_TEXT: Rgba = Rgba::rgb(0x9C, 0xBB, 0xFF);

    /// An input port pulling from the depot (and a source node, which does the same).
    pub const DEPOT_IN: Rgba = Rgba::rgb(0xF0, 0x7A, 0xAE);
    /// An output port sending to the depot (and a sink node, which does the same).
    pub const DEPOT_OUT: Rgba = Rgba::rgb(0x6C, 0xC7, 0x7E);
    /// A linked port, and links.
    pub const LINKED: Rgba = Rgba::rgb(0x9A, 0xA0, 0xAB);
    /// Throttle markers and hints such as a link over one belt.
    pub const WARN: Rgba = Rgba::rgb(0xE9, 0xA2, 0x3B);
    /// A storage filling up.
    pub const POSITIVE: Rgba = Rgba::rgb(0x6C, 0xC7, 0x7E);
    /// A storage draining, text that doesn't parse, errors.
    pub const NEGATIVE: Rgba = Rgba::rgb(0xEF, 0x64, 0x61);
    /// The box a right-drag draws to select.
    pub const SELECT_BOX: Rgba = ACCENT;

    /// Node card headers: a low-chroma tint per node kind, so kinds tell apart at a glance while
    /// the header text stays the loudest thing on it.
    pub mod node {
        use crate::style::Rgba;

        pub const RECIPE: Rgba = Rgba::rgb(0x28, 0x33, 0x49);
        /// Pink family, like a port pulling from the depot.
        pub const SOURCE: Rgba = Rgba::rgb(0x4A, 0x2A, 0x3C);
        /// Green family, like a port sending to the depot.
        pub const SINK: Rgba = Rgba::rgb(0x21, 0x3A, 0x29);
        pub const OUTPOST_ORDER: Rgba = Rgba::rgb(0x45, 0x36, 0x1C);
        pub const MISSING: Rgba = Rgba::rgb(0x52, 0x24, 0x26);
        /// Splitters, mergers, storage, control ports and workspace ports.
        pub const LOGISTICS: Rgba = Rgba::rgb(0x2E, 0x30, 0x36);
        pub const WORKSPACE: Rgba = Rgba::rgb(0x3A, 0x30, 0x52);
    }

    /// Region badges: one saturated tint per region, in the data's region order (Valley IV, then
    /// Wuling), so a region reads the same everywhere and two regions tell apart without reading.
    /// Yellow and teal; the yellow is a cooler lemon than the warning's amber.
    pub mod region {
        use crate::style::Rgba;

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct Tint {
            pub fill: Rgba,
            pub stroke: Rgba,
            pub text: Rgba,
        }

        pub const TINTS: [Tint; 2] = [
            Tint {
                fill: Rgba::rgb(0x3B, 0x36, 0x0E),
                stroke: Rgba::rgb(0x75, 0x6A, 0x1B),
                text: Rgba::rgb(0xF2, 0xE0, 0x5A),
            },
            Tint {
                fill: Rgba::rgb(0x12, 0x3D, 0x40),
                stroke: Rgba::rgb(0x23, 0x6E, 0x73),
                text: Rgba::rgb(0x74, 0xE0, 0xE4),
            },
        ];
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
    /// Numbers in panels, port rates: the tabular face (`Monospace`).
    pub const MONO: f32 = 13.0;
    /// Panel titles.
    pub const HEADING: f32 = 15.0;
    /// The number a card is about: its facility count, a source's supply.
    pub const NUMBER: f32 = 18.0;
}

/// Spacing, in points.
pub mod spacing {
    /// Between neighbouring widgets: across, down.
    pub const ITEM: [f32; 2] = [8.0, 3.0];
    /// Inside a button: across, down.
    pub const BUTTON_PADDING: [f32; 2] = [8.0, 2.0];
    /// The smallest a widget that can be clicked is: across, down.
    pub const INTERACT_SIZE: [f32; 2] = [40.0, 20.0];
    /// Inside a window.
    pub const WINDOW_MARGIN: i8 = 10;
    /// Inside a menu.
    pub const MENU_MARGIN: i8 = 4;
    /// Inside a node card.
    pub const CARD_MARGIN: i8 = 6;
    /// One step of indentation.
    pub const INDENT: f32 = 16.0;
    /// A checkbox or radio button.
    pub const ICON_WIDTH: f32 = 14.0;
    /// The mark inside it.
    pub const ICON_WIDTH_INNER: f32 = 8.0;
    /// Between it and its label.
    pub const ICON_SPACING: f32 = 6.0;
    /// The tallest a dropdown list grows.
    pub const COMBO_HEIGHT: f32 = 320.0;
}

/// A scroll bar's handle: the text color, this opaque over its `INSET` track.
pub mod scroll {
    /// The pointer is elsewhere.
    pub const HANDLE: f32 = 0.5;
    /// The pointer is over what scrolls.
    pub const HANDLE_NEAR: f32 = 0.65;
    /// The pointer is on the bar or drags it.
    pub const HANDLE_HELD: f32 = 0.85;
}

/// A drop shadow: where it falls, how soft it is, how dark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shadow {
    pub offset: [i8; 2],
    pub blur: u8,
    /// Black, this opaque (0 to 255).
    pub alpha: u8,
}

/// Shadows. Panels and bars carry none.
pub mod shadow {
    use super::Shadow;

    /// A node card.
    pub const CARD: Shadow = Shadow {
        offset: [0, 2],
        blur: 6,
        alpha: 90,
    };
    /// A popover, a menu.
    pub const POPOVER: Shadow = Shadow {
        offset: [0, 4],
        blur: 14,
        alpha: 120,
    };
    /// A window.
    pub const WINDOW: Shadow = Shadow {
        offset: [0, 8],
        blur: 24,
        alpha: 140,
    };
}

/// The app's faces, embedded so it looks the same on macOS and Windows, under the SIL Open Font
/// License (`fonts/Inter-LICENSE.txt`). Inter Display carries words in static Light and SemiBold
/// cuts. Numbers that must line up use the same cuts with tabular figures frozen in as the default
/// digits (`scripts/tabular-fonts.sh`), for toolkits that shape text without OpenType features.
pub mod fonts {
    pub const INTER_LIGHT: &[u8] = include_bytes!("../fonts/InterDisplay-Light.ttf");
    pub const INTER_SEMIBOLD: &[u8] = include_bytes!("../fonts/InterDisplay-SemiBold.ttf");
    pub const INTER_TABULAR_LIGHT: &[u8] = include_bytes!("../fonts/InterDisplayTabular-Light.ttf");
    pub const INTER_TABULAR_SEMIBOLD: &[u8] =
        include_bytes!("../fonts/InterDisplayTabular-SemiBold.ttf");
    /// The license text of all four.
    pub const LICENSE: &str = include_str!("../fonts/Inter-LICENSE.txt");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Relative luminance (WCAG 2), for contrast checks.
    fn luminance(c: Rgba) -> f32 {
        let channel = |v: u8| {
            let v = f32::from(v) / 255.0;
            if v <= 0.039_28 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(c.0) + 0.7152 * channel(c.1) + 0.0722 * channel(c.2)
    }

    fn contrast(a: Rgba, b: Rgba) -> f32 {
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
    fn a_scroll_handle_stands_out_from_its_track() {
        let over = |top: u8, under: u8| {
            (f32::from(under) + (f32::from(top) - f32::from(under)) * scroll::HANDLE) as u8
        };
        let (top, track) = (color::TEXT, color::INSET);
        let handle = Rgba::rgb(
            over(top.0, track.0),
            over(top.1, track.1),
            over(top.2, track.2),
        );
        for surface in [track, color::PANEL, color::RAISED] {
            assert!(contrast(handle, surface) >= 3.0, "on {surface:?}");
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
    fn every_font_is_a_truetype_file() {
        for font in [
            fonts::INTER_LIGHT,
            fonts::INTER_SEMIBOLD,
            fonts::INTER_TABULAR_LIGHT,
            fonts::INTER_TABULAR_SEMIBOLD,
        ] {
            assert_eq!(font[..4], [0x00, 0x01, 0x00, 0x00]);
        }
        assert!(fonts::LICENSE.contains("Open Font License"));
    }
}
