//! The tutorial: a small window over the canvas that teaches adding a node, growing a chain, the
//! keys and saving. It opens by itself at a start with nothing to reopen (see
//! `aic_editor::tutorial::opens_at_start`), and the top bar's Tutorial button opens and closes it.
//! It is not modal: the canvas around and under it keeps working.
//!
//! Each row's picture is a tiny drawing of the app's own objects, made with the painter so the
//! picture is the gesture. Coordinates are whole points from the well's corner; a 1pt line sits on
//! a half point so it covers one pixel at 1x.

use std::sync::Arc;

use eframe::egui::emath::GuiRounding as _;
use eframe::egui::{
    self, Align2, CornerRadius, Galley, Painter, Pos2, Rect, RichText, Sense, Shape, Stroke,
    StrokeKind, TextStyle, TextWrapMode, Ui, Vec2, WidgetText, pos2, vec2,
};

use crate::editor::Editor;
use crate::i18n::Strings;
use crate::theme::{self, color, radius, text};
use aic_editor::{Command, tutorial};

/// The content's width, inside the window's margins.
const WIDTH: f32 = 300.0;
/// The well each row's picture sits in.
const PICTURE: Vec2 = vec2(56.0, 44.0);
/// Between a picture and its row's text.
const PICTURE_GAP: f32 = 10.0;
/// Between two rows: more than between a row's title and its text.
const ROW_GAP: f32 = 12.0;
/// Between a row's title and its text, and between its lines.
const LINE_GAP: f32 = 3.0;
/// How far inside the canvas's bottom-right corner the window's own corner first sits.
const CANVAS_INSET: f32 = 16.0;
/// Around a key cap's text.
const KEY_PADDING: Vec2 = vec2(4.0, 1.0);
/// Between a command's name and its key cap.
const KEY_GAP: f32 = 6.0;
/// Between one name and cap and the next on a line.
const PAIR_GAP: f32 = 14.0;
/// A wire, as thick as on the canvas.
const WIRE: f32 = 2.0;
/// The letter on the Keys picture's front cap: part of the drawing, not a word to translate.
const KEY_LETTER: &str = "C";

#[derive(Clone, Copy)]
enum Picture {
    AddNode,
    Grow,
    Keys,
    Save,
}

/// The top bar button's picture: a symbol egui's built-in fonts draw, as the toolbar's are
/// (checked by eye in a snapshot).
const ICON: &str = "🎓";

impl Editor {
    /// The top bar's Tutorial button: lit while the window is open; a click opens or closes it.
    pub(crate) fn tutorial_toggle(&mut self, ui: &mut Ui, strings: &Strings) {
        let name = strings.get("toggle-tutorial");
        crate::widgets::icon_toggle(ui, &mut self.show_tutorial, ICON, name);
    }

    /// The tutorial window, while it is open. It waits for the canvas to be laid out, so it first
    /// shows in the canvas's bottom-right corner.
    pub(crate) fn tutorial_ui(&mut self, ctx: &egui::Context, strings: &Strings) {
        let canvas = self.canvas.rect;
        if !self.show_tutorial || !canvas.is_positive() {
            return;
        }
        let mut open = true;
        egui::Window::new(strings.get("tutorial-title"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .pivot(Align2::RIGHT_BOTTOM)
            .default_pos(canvas.right_bottom() - Vec2::splat(CANVAS_INSET))
            .show(ctx, |ui| self.tutorial_rows(ui, strings));
        self.show_tutorial = open;
    }

    fn tutorial_rows(&self, ui: &mut Ui, strings: &Strings) {
        ui.set_width(WIDTH);
        ui.spacing_mut().item_spacing.y = ROW_GAP;
        row(
            ui,
            Picture::AddNode,
            strings.get("tutorial-add-title"),
            |ui| {
                body(ui, strings.get("tutorial-add-body"));
            },
        );
        row(
            ui,
            Picture::Grow,
            strings.get("tutorial-grow-title"),
            |ui| {
                body(ui, strings.get("tutorial-grow-body"));
            },
        );
        row(
            ui,
            Picture::Keys,
            strings.get("tutorial-keys-title"),
            |ui| {
                key_lines(ui, strings, &tutorial::KEYS);
            },
        );
        row(
            ui,
            Picture::Save,
            strings.get("tutorial-save-title"),
            |ui| {
                key_lines(ui, strings, &[&[tutorial::SAVE]]);
                body(ui, strings.get(tutorial::saving_key(&self.session)));
                let export = strings.format(
                    "tutorial-export",
                    &[
                        ("menu", &strings.get("menu-file")),
                        ("item", &strings.get("menu-export-png")),
                    ],
                );
                body(ui, export);
            },
        );
    }
}

/// A picture on the left; on the right a title over what `contents` adds.
fn row(ui: &mut Ui, picture: Picture, title: String, contents: impl FnOnce(&mut Ui)) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = PICTURE_GAP;
        well(ui, picture);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = LINE_GAP;
            ui.label(theme::strong(title).size(text::BODY));
            contents(ui);
        });
    });
}

/// Body text, wrapped to the row.
fn body(ui: &mut Ui, words: String) {
    ui.label(RichText::new(words).size(text::BODY).color(color::TEXT));
}

/// Each command's name with its key cap, a line for each slice of `lines`. Names and caps line up
/// in columns, and a name sits on its cap's baseline.
fn key_lines(ui: &mut Ui, strings: &Strings, lines: &[&[Command]]) {
    let shape = |ui: &Ui, words: String, size: f32| {
        WidgetText::from(RichText::new(words).size(size)).into_galley(
            ui,
            Some(TextWrapMode::Extend),
            f32::INFINITY,
            TextStyle::Body,
        )
    };
    let lines: Vec<Vec<KeyPair>> = lines
        .iter()
        .map(|line| {
            line.iter()
                .map(|&command| {
                    let keys = ui.ctx().format_shortcut(&crate::edit::keys(command));
                    KeyPair {
                        name: shape(ui, strings.get(command.label_key()), text::BODY),
                        keys: shape(ui, keys, text::SMALL),
                    }
                })
                .collect()
        })
        .collect();
    let columns = lines.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![(0.0_f32, 0.0_f32); columns];
    for line in &lines {
        for (width, pair) in widths.iter_mut().zip(line) {
            width.0 = width.0.max(pair.name.size().x.ceil());
            width.1 = width.1.max(pair.keys.size().x.ceil());
        }
    }
    // Inside a cap: its padding and its outline.
    let inset = KEY_PADDING + Vec2::splat(1.0);
    let height = lines
        .iter()
        .flatten()
        .map(|pair| pair.keys.size().y)
        .fold(0.0, f32::max)
        .ceil()
        + inset.y * 2.0;
    for line in lines {
        let spoken = line
            .iter()
            .map(|pair| format!("{} {}", pair.name.text(), pair.keys.text()))
            .collect::<Vec<_>>()
            .join(", ");
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Label, true, spoken.as_str())
        });
        if !ui.is_rect_visible(rect) {
            continue;
        }
        let painter = ui.painter();
        let mut x = rect.left().round();
        let top = rect.top().round();
        for (pair, (name_width, keys_width)) in line.into_iter().zip(&widths) {
            let keys_at = pos2(x + name_width + KEY_GAP + inset.x, top + inset.y);
            let baseline = keys_at.y + baseline(&pair.keys);
            painter.galley(
                pos2(x, (baseline - self::baseline(&pair.name)).round()),
                pair.name,
                color::TEXT,
            );
            let cap = Rect::from_min_size(
                pos2(x + name_width + KEY_GAP, top),
                vec2(pair.keys.size().x.ceil() + inset.x * 2.0, height),
            );
            painter.rect(
                cap,
                radius::SMALL,
                color::CONTROL,
                Stroke::new(1.0, color::BORDER),
                StrokeKind::Inside,
            );
            painter.galley(keys_at, pair.keys, color::TEXT);
            x += name_width + KEY_GAP + keys_width + inset.x * 2.0 + PAIR_GAP;
        }
    }
}

/// A command's name and the keys that run it, shaped.
struct KeyPair {
    name: Arc<Galley>,
    keys: Arc<Galley>,
}

/// How far below a galley's top its first line's baseline is.
fn baseline(galley: &Galley) -> f32 {
    galley
        .rows
        .first()
        .and_then(|row| row.row.glyphs.first().map(|glyph| row.pos.y + glyph.pos.y))
        .unwrap_or(galley.size().y)
}

/// A row's picture in its well.
fn well(ui: &mut Ui, picture: Picture) {
    let (rect, _) = ui.allocate_exact_size(PICTURE, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let rect = rect.round_to_pixels(painter.pixels_per_point());
    painter.rect(
        rect,
        radius::SMALL,
        color::INSET,
        Stroke::new(1.0, color::DIVIDER),
        StrokeKind::Inside,
    );
    let sketch = Sketch {
        painter,
        origin: rect.min,
    };
    match picture {
        Picture::AddNode => sketch.add_node(),
        Picture::Grow => sketch.grow(),
        Picture::Keys => sketch.keys(),
        Picture::Save => sketch.save(),
    }
}

/// Draws in a well, in whole points from its top-left corner.
struct Sketch<'a> {
    painter: &'a Painter,
    origin: Pos2,
}

impl Sketch<'_> {
    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.origin + vec2(x, y)
    }

    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_min_max(self.at(x0, y0), self.at(x1, y1))
    }

    /// A box with a 1pt outline inside its edges.
    fn panel(&self, rect: Rect, radius: impl Into<CornerRadius>, fill: egui::Color32) {
        self.painter.rect(
            rect,
            radius,
            fill,
            Stroke::new(1.0, color::BORDER),
            StrokeKind::Inside,
        );
    }

    /// A 1pt line along row `y`, from `x0` to `x1`.
    fn line(&self, x0: f32, x1: f32, y: f32, color: egui::Color32) {
        self.painter.line_segment(
            [self.at(x0, y + 0.5), self.at(x1, y + 0.5)],
            Stroke::new(1.0, color),
        );
    }

    /// A shape through `points` (on half points, so its outline is crisp).
    fn polygon(&self, points: &[(f32, f32)], fill: egui::Color32, stroke: Stroke) {
        let points = points
            .iter()
            .map(|&(x, y)| self.at(x + 0.5, y + 0.5))
            .collect();
        self.painter
            .add(Shape::convex_polygon(points, fill, stroke));
    }

    /// The pointer beside the node picker it opened: a search field over a few recipe rows.
    fn add_node(&self) {
        self.panel(
            self.rect(20.0, 8.0, 50.0, 38.0),
            radius::SMALL,
            color::RAISED,
        );
        self.painter
            .rect_filled(self.rect(23.0, 11.0, 47.0, 17.0), 1, color::INSET);
        self.line(25.0, 33.0, 13.0, color::TEXT_FAINT);
        for (y, end) in [(21.0, 44.0), (26.0, 40.0), (31.0, 42.0)] {
            self.line(24.0, end, y, color::TEXT_MUTED);
        }
        let outline = Stroke::new(1.0, color::INSET);
        self.polygon(
            &[(8.0, 8.0), (8.0, 19.0), (15.0, 15.0)],
            color::TEXT_STRONG,
            outline,
        );
        self.painter.line_segment(
            [self.at(10.5, 15.5), self.at(13.0, 21.0)],
            Stroke::new(WIRE, color::TEXT_STRONG),
        );
    }

    /// A recipe card's output port with a wire dragged out to the node it will add.
    fn grow(&self) {
        let card = self.rect(5.0, 10.0, 23.0, 34.0);
        self.panel(card, radius::SMALL, color::RAISED);
        let corner = radius::SMALL - 1;
        self.painter.rect_filled(
            self.rect(6.0, 11.0, 22.0, 17.0),
            CornerRadius {
                nw: corner,
                ne: corner,
                sw: 0,
                se: 0,
            },
            color::node::RECIPE,
        );
        self.line(8.0, 17.0, 21.0, color::TEXT_MUTED);
        self.line(8.0, 14.0, 25.0, color::TEXT_FAINT);
        let pin = self.at(23.0, 26.0);
        self.painter
            .line_segment([pin, self.at(35.0, 26.0)], Stroke::new(WIRE, color::LINKED));
        self.painter.circle_filled(pin, 2.5, color::LINKED);
        let ghost = [
            (35.0, 14.0),
            (51.0, 14.0),
            (51.0, 38.0),
            (35.0, 38.0),
            (35.0, 14.0),
        ]
        .map(|(x, y)| self.at(x + 0.5, y + 0.5));
        self.painter.extend(Shape::dashed_line(
            &ghost,
            Stroke::new(1.0, color::BORDER_STRONG),
            2.0,
            2.0,
        ));
    }

    /// Two key caps, one in front of the other; the front one carries a letter, so the pair reads
    /// as keys and not as two boxes.
    fn keys(&self) {
        self.key(8.0, 8.0, 22.0, 20.0);
        self.key(25.0, 16.0, 24.0, 20.0);
        // The middle of the front cap's face.
        self.painter.text(
            self.at(37.0, 24.5),
            Align2::CENTER_CENTER,
            KEY_LETTER,
            egui::FontId::new(text::SMALL, theme::semibold()),
            color::TEXT_MUTED,
        );
    }

    fn key(&self, x: f32, y: f32, w: f32, h: f32) {
        self.panel(self.rect(x, y, x + w, y + h), radius::SMALL, color::CONTROL);
        self.painter.rect_filled(
            self.rect(x + 2.0, y + 2.0, x + w - 2.0, y + h - 5.0),
            radius::SMALL / 2,
            color::CONTROL_HOVER,
        );
    }

    /// A file with a folded corner, and a picture of the canvas in front of it.
    fn save(&self) {
        let stroke = Stroke::new(1.0, color::BORDER);
        self.polygon(
            &[
                (11.0, 6.0),
                (25.0, 6.0),
                (31.0, 12.0),
                (31.0, 37.0),
                (11.0, 37.0),
            ],
            color::RAISED,
            stroke,
        );
        self.polygon(
            &[(25.0, 6.0), (25.0, 12.0), (31.0, 12.0)],
            color::CONTROL_ACTIVE,
            stroke,
        );
        for (y, end) in [(16.0, 26.0), (20.0, 28.0), (24.0, 22.0)] {
            self.line(14.0, end, y, color::TEXT_FAINT);
        }
        let frame = self.rect(25.0, 21.0, 48.0, 39.0);
        self.painter.rect(
            frame,
            1,
            color::RAISED,
            Stroke::new(1.0, color::BORDER_STRONG),
            StrokeKind::Inside,
        );
        self.polygon(
            &[(27.0, 36.0), (33.0, 28.0), (39.0, 36.0)],
            color::TEXT_FAINT,
            Stroke::NONE,
        );
        self.polygon(
            &[(35.0, 36.0), (40.0, 30.0), (45.0, 36.0)],
            color::TEXT_MUTED,
            Stroke::NONE,
        );
        self.painter
            .circle_filled(self.at(42.5, 25.5), 1.5, color::TEXT_MUTED);
    }
}
