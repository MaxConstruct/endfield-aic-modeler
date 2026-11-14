//! The design system on one screen: every token in `aic_app_egui::theme` and the standard widgets in the
//! app's style, with a canvas sample and the overlays. A developer tool, so its strings are not
//! translated.
//!
//! `cargo run -p aic-app-egui --example showcase`

use aic_app_egui::theme::{self, color, radius, text};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind,
    Ui, Vec2, epaint::CubicBezierShape, pos2, vec2,
};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AIC Modeler design system")
            .with_inner_size([1440.0, 960.0]),
        ..Default::default()
    };
    eframe::run_native(
        "AIC Modeler design system",
        options,
        Box::new(|cc| {
            theme::install(&cc.egui_ctx);
            Ok(Box::new(Showcase::default()))
        }),
    )
}

pub struct Showcase {
    query: String,
    limit: String,
    bad_limit: String,
    recipes: bool,
    inspector: bool,
    round_up: bool,
    facility: usize,
    matching: [bool; 3],
}

impl Default for Showcase {
    fn default() -> Self {
        Self {
            query: String::new(),
            limit: "225".into(),
            bad_limit: "22x".into(),
            recipes: true,
            inspector: true,
            round_up: false,
            facility: 0,
            matching: [true, false, true],
        }
    }
}

impl eframe::App for Showcase {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

const FACILITIES: [&str; 2] = ["Reactor Crucible", "Reactor Crucible Mk II"];

impl Showcase {
    pub fn show(&mut self, ui: &mut Ui) {
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    for (label, key) in [("New", "Cmd+N"), ("Open…", "Cmd+O"), ("Save", "Cmd+S")]
                    {
                        ui.add(egui::Button::new(label).shortcut_text(key));
                    }
                });
                ui.menu_button("Edit", |ui| {
                    for (label, key) in [("Undo", "Cmd+Z"), ("Duplicate", "Cmd+D")] {
                        ui.add(egui::Button::new(label).shortcut_text(key));
                    }
                });
                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.round_up, "Round up");
                });
                ui.separator();
                ui.toggle_value(&mut self.recipes, "Recipes");
                ui.label(RichText::new("Region").color(color::TEXT_MUTED));
                ui.label("Wuling");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.toggle_value(&mut self.inspector, "Inspector");
                });
            });
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new("Saved")
                        .size(text::SMALL)
                        .color(color::TEXT_MUTED),
                );
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.sections(ui);
            });
        });
    }

    fn sections(&mut self, ui: &mut Ui) {
        section(ui, "Surfaces");
        swatches(
            ui,
            &[
                ("CANVAS", color::CANVAS),
                ("PANEL", color::PANEL),
                ("RAISED", color::RAISED),
                ("INSET", color::INSET),
                ("STRIPE", color::STRIPE),
                ("CONTROL", color::CONTROL),
                ("CONTROL_HOVER", color::CONTROL_HOVER),
                ("CONTROL_ACTIVE", color::CONTROL_ACTIVE),
            ],
        );
        swatches(
            ui,
            &[
                ("GRID", color::GRID),
                ("DIVIDER", color::DIVIDER),
                ("BORDER", color::BORDER),
                ("BORDER_STRONG", color::BORDER_STRONG),
            ],
        );

        section(ui, "Text");
        ui.horizontal(|ui| {
            for (name, c) in [
                ("TEXT_STRONG", color::TEXT_STRONG),
                ("TEXT", color::TEXT),
                ("TEXT_MUTED", color::TEXT_MUTED),
                ("TEXT_FAINT", color::TEXT_FAINT),
            ] {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Cuprium Ore 30.00").color(c));
                    caption(ui, name, c);
                });
                ui.add_space(24.0);
            }
        });

        section(ui, "Accent and states");
        swatches(
            ui,
            &[
                ("ACCENT", color::ACCENT),
                ("ACCENT_FILL", color::ACCENT_FILL),
                ("ACCENT_TEXT", color::ACCENT_TEXT),
                ("DEPOT_IN", color::DEPOT_IN),
                ("DEPOT_OUT", color::DEPOT_OUT),
                ("LINKED", color::LINKED),
                ("WARN", color::WARN),
                ("POSITIVE", color::POSITIVE),
                ("NEGATIVE", color::NEGATIVE),
            ],
        );

        section(ui, "Node kinds");
        ui.horizontal(|ui| {
            use color::node::*;
            for (name, tint) in [
                ("Recipe", RECIPE),
                ("Source", SOURCE),
                ("Sink", SINK),
                ("Outpost order", OUTPOST_ORDER),
                ("Missing recipe", MISSING),
                ("Logistics", LOGISTICS),
                ("Workspace", WORKSPACE),
            ] {
                let (rect, _) = ui.allocate_exact_size(vec2(128.0, 28.0), Sense::hover());
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(radius::MEDIUM), tint);
                ui.painter().text(
                    rect.left_center() + vec2(10.0, 0.0),
                    Align2::LEFT_CENTER,
                    name,
                    FontId::new(text::BODY, theme::semibold()),
                    color::TEXT_STRONG,
                );
            }
        });

        section(ui, "Type");
        egui::Grid::new("type")
            .num_columns(2)
            .spacing([24.0, 8.0])
            .show(ui, |ui| {
                let rows: [(&str, RichText); 5] = [
                    (
                        "Number · mono semibold 18",
                        RichText::new("1.75  30.00")
                            .font(theme::number(text::NUMBER))
                            .color(color::TEXT_STRONG),
                    ),
                    (
                        "Heading · Inter semibold 15",
                        theme::strong("Reactor Crucible").size(text::HEADING),
                    ),
                    ("Strong · Inter semibold 13", theme::strong("Ports")),
                    (
                        "Body · Inter 13",
                        RichText::new("Clean Water from the depot"),
                    ),
                    (
                        "Mono · 13",
                        RichText::new("120.00 /min  0.40 belts").monospace(),
                    ),
                ];
                for (name, sample) in rows {
                    caption(ui, name, color::TEXT_MUTED);
                    ui.label(sample);
                    ui.end_row();
                }
                caption(ui, "Small · Inter 12", color::TEXT_MUTED);
                ui.label(RichText::new("30.00 /min · 1 belts").size(text::SMALL));
                ui.end_row();
            });

        section(ui, "Controls");
        ui.horizontal(|ui| {
            let w = &ui.visuals().widgets.clone();
            state_chip(
                ui,
                "Rest",
                w.inactive.weak_bg_fill,
                w.inactive.bg_stroke,
                w.inactive.fg_stroke.color,
            );
            state_chip(
                ui,
                "Hover",
                w.hovered.weak_bg_fill,
                w.hovered.bg_stroke,
                w.hovered.fg_stroke.color,
            );
            state_chip(
                ui,
                "Pressed",
                w.active.weak_bg_fill,
                w.active.bg_stroke,
                w.active.fg_stroke.color,
            );
            state_chip(
                ui,
                "Selected",
                color::ACCENT_FILL,
                Stroke::NONE,
                color::ACCENT_TEXT,
            );
            let disabled = ui.visuals().disabled_alpha;
            state_chip(
                ui,
                "Disabled",
                w.inactive.weak_bg_fill.gamma_multiply(disabled),
                Stroke::new(1.0, w.inactive.bg_stroke.color.gamma_multiply(disabled)),
                w.inactive.fg_stroke.color.gamma_multiply(disabled),
            );
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let _ = ui.button("Remove node");
            ui.add_enabled(false, egui::Button::new("Paste"));
            for (on, label) in self.matching.iter_mut().zip(["Name", "Inputs", "Outputs"]) {
                ui.toggle_value(on, label);
            }
            ui.checkbox(&mut self.round_up, "Round up");
            egui::ComboBox::from_id_salt("facility")
                .selected_text(FACILITIES[self.facility])
                .show_ui(ui, |ui| {
                    for (i, name) in FACILITIES.iter().enumerate() {
                        ui.selectable_value(&mut self.facility, i, *name);
                    }
                });
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text("Search recipes")
                    .desired_width(220.0),
            );
            ui.label("Limit");
            ui.add(
                egui::TextEdit::singleline(&mut self.limit)
                    .horizontal_align(egui::Align::Center)
                    .desired_width(64.0),
            );
            ui.label(RichText::new("/min").color(color::TEXT_MUTED));
            ui.add(
                egui::TextEdit::singleline(&mut self.bad_limit)
                    .horizontal_align(egui::Align::Center)
                    .text_color(color::NEGATIVE)
                    .desired_width(64.0),
            );
            caption(ui, "doesn't parse", color::TEXT_FAINT);
        });

        section(ui, "Canvas");
        canvas_sample(ui);

        section(ui, "Overlays");
        ui.horizontal_top(|ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(220.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(theme::strong("Limit"));
                        let mut rate = String::from("225");
                        ui.add(egui::TextEdit::singleline(&mut rate).desired_width(64.0));
                        ui.label(RichText::new("/min").color(color::TEXT_MUTED));
                    });
                    let _ = ui.button("Clear connections");
                });
            });
            ui.add_space(24.0);
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_width(200.0);
                ui.vertical(|ui| {
                    for (label, key) in [("Open", ""), ("Duplicate", "Cmd+D"), ("Delete", "Del")] {
                        ui.add(egui::Button::new(label).shortcut_text(key).frame(false));
                    }
                });
            });
            ui.add_space(24.0);
            egui::Frame::window(ui.style()).show(ui, |ui| {
                ui.set_width(280.0);
                ui.vertical(|ui| {
                    ui.label(theme::strong("Settings").size(text::HEADING));
                    ui.separator();
                    egui::Grid::new("settings").num_columns(2).show(ui, |ui| {
                        ui.label("Data version");
                        ui.label(RichText::new("v1.5.3").monospace());
                        ui.end_row();
                        ui.label("Game names");
                        ui.label("en");
                        ui.end_row();
                    });
                });
            });
            ui.add_space(24.0);
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(RichText::new("Add a recipe node").size(text::SMALL));
            });
        });
        ui.add_space(24.0);
    }
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(20.0);
    ui.label(theme::strong(title).size(text::HEADING));
    ui.separator();
    ui.add_space(4.0);
}

fn caption(ui: &mut Ui, label: &str, c: Color32) {
    ui.label(RichText::new(label).size(text::SMALL).color(c));
}

/// One swatch per color, named, with its hex below.
fn swatches(ui: &mut Ui, colors: &[(&str, Color32)]) {
    ui.horizontal_wrapped(|ui| {
        for (name, c) in colors {
            ui.vertical(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(112.0, 44.0), Sense::hover());
                ui.painter().rect(
                    rect,
                    CornerRadius::same(radius::SMALL),
                    *c,
                    Stroke::new(1.0, color::BORDER),
                    StrokeKind::Inside,
                );
                caption(ui, name, color::TEXT);
                let hex = format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b());
                ui.label(
                    RichText::new(hex)
                        .size(text::SMALL)
                        .monospace()
                        .color(color::TEXT_FAINT),
                );
            });
            ui.add_space(4.0);
        }
    });
}

/// A button painted in one interaction state, so every state shows at once.
fn state_chip(ui: &mut Ui, label: &str, fill: Color32, stroke: Stroke, ink: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(96.0, 20.0), Sense::hover());
    ui.painter().rect(
        rect,
        CornerRadius::same(radius::SMALL),
        fill,
        stroke,
        StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(text::BODY),
        ink,
    );
}

/// A canvas strip with a source feeding a selected recipe card, as the app draws them.
fn canvas_sample(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 280.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(radius::MEDIUM), color::CANVAS);
    let mut x = rect.left() + 40.0;
    while x < rect.right() {
        painter.vline(x, rect.y_range(), Stroke::new(1.0, color::GRID));
        x += 40.0;
    }
    let mut y = rect.top() + 40.0;
    while y < rect.bottom() {
        painter.hline(rect.x_range(), y, Stroke::new(1.0, color::GRID));
        y += 40.0;
    }

    let source = Rect::from_min_size(rect.min + vec2(60.0, 60.0), vec2(220.0, 140.0));
    let recipe = Rect::from_min_size(rect.min + vec2(460.0, 40.0), vec2(300.0, 170.0));
    let out = pos2(source.right() - 14.0, source.top() + 44.0);
    let input = pos2(recipe.left() + 14.0, recipe.top() + 44.0);

    let wire = Stroke::new(2.0, color::LINKED);
    painter.add(CubicBezierShape::from_points_stroke(
        [out, out + vec2(90.0, 0.0), input - vec2(90.0, 0.0), input],
        false,
        Color32::TRANSPARENT,
        wire,
    ));
    let mid = out.lerp(input, 0.5);
    let label = painter.layout_no_wrap(
        "30.00 /min · 1 belts".into(),
        FontId::proportional(text::SMALL),
        color::TEXT,
    );
    let label_rect = Rect::from_center_size(mid, label.size()).expand(3.0);
    painter.rect_filled(label_rect, 3.0, color::INSET.gamma_multiply(0.9));
    painter.galley(label_rect.min + vec2(3.0, 3.0), label, color::TEXT);

    card(
        &painter,
        source,
        color::node::SOURCE,
        Some("Source"),
        "Cuprium Ore",
        false,
    );
    pin(&painter, out, "CO", Color32::from_rgb(0xD9, 0x8A, 0x5B));
    rate(&painter, out, "30.00", color::LINKED, false);
    big_number(&painter, source, "30.00", "30");

    card(&painter, recipe, color::node::RECIPE, None, "Cuprium", true);
    pin(&painter, input, "CO", Color32::from_rgb(0xD9, 0x8A, 0x5B));
    rate(&painter, input, "30.00", color::LINKED, true);
    let water = input + vec2(0.0, 28.0);
    pin(&painter, water, "CW", Color32::from_rgb(0x8F, 0xB8, 0xE8));
    rate(&painter, water, "30.00", color::DEPOT_IN, true);
    let product = pos2(recipe.right() - 14.0, input.y);
    pin(&painter, product, "C", Color32::from_rgb(0xE0, 0x7B, 0x4F));
    rate(&painter, product, "30.00", color::DEPOT_OUT, false);
    let sewage = product + vec2(0.0, 28.0);
    pin(&painter, sewage, "S", Color32::from_rgb(0xA8, 0xC0, 0x8A));
    rate(&painter, sewage, "30.00", color::WARN, false);
    throttle_marker(&painter, sewage);
    painter.text(
        pos2(recipe.center().x, recipe.top() + 84.0),
        Align2::CENTER_CENTER,
        "Refining Unit",
        FontId::proportional(text::SMALL),
        color::TEXT,
    );
    big_number(&painter, recipe, "1.00", "no limit");
}

fn card(
    painter: &egui::Painter,
    rect: Rect,
    tint: Color32,
    kind: Option<&str>,
    title: &str,
    selected: bool,
) {
    let frame = theme::node_frame();
    let corner = CornerRadius::same(radius::MEDIUM);
    painter.add(frame.shadow.as_shape(rect, corner));
    painter.rect_filled(rect, corner, frame.fill);
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), 26.0));
    let top = CornerRadius {
        nw: radius::MEDIUM,
        ne: radius::MEDIUM,
        sw: 0,
        se: 0,
    };
    painter.rect_filled(header, top, tint);
    let mut at = header.left_center() + vec2(10.0, 0.0);
    if let Some(kind) = kind {
        let galley = painter.layout_no_wrap(
            kind.into(),
            FontId::proportional(text::SMALL),
            color::TEXT_MUTED,
        );
        let width = galley.size().x;
        painter.galley(
            at - vec2(0.0, galley.size().y / 2.0),
            galley,
            color::TEXT_MUTED,
        );
        at.x += width + 8.0;
    }
    painter.text(
        at,
        Align2::LEFT_CENTER,
        title,
        FontId::new(text::BODY, theme::semibold()),
        color::TEXT_STRONG,
    );
    let outline = if selected {
        Stroke::new(2.0, color::ACCENT)
    } else {
        frame.stroke
    };
    painter.rect_stroke(rect, corner, outline, StrokeKind::Inside);
}

/// An item pin: a badge with the item's initials, as when it has no icon.
fn pin(painter: &egui::Painter, center: Pos2, initials: &str, fill: Color32) {
    let rect = Rect::from_center_size(center, Vec2::splat(20.0));
    painter.rect_filled(rect, 3.0, fill);
    painter.text(
        center,
        Align2::CENTER_CENTER,
        initials,
        FontId::proportional(9.0),
        Color32::BLACK,
    );
}

fn rate(painter: &egui::Painter, pin: Pos2, value: &str, c: Color32, input: bool) {
    let (pos, align) = if input {
        (pin + vec2(16.0, 0.0), Align2::LEFT_CENTER)
    } else {
        (pin - vec2(16.0, 0.0), Align2::RIGHT_CENTER)
    };
    painter.text(pos, align, value, FontId::monospace(text::MONO), c);
}

fn throttle_marker(painter: &egui::Painter, pin: Pos2) {
    let tile = Rect::from_center_size(pin, Vec2::splat(20.0));
    let r = Rect::from_min_size(tile.right_top() - vec2(9.0, 3.0), vec2(9.0, 8.0));
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(r.center().x, r.top()),
            r.right_bottom(),
            r.left_bottom(),
        ],
        color::WARN,
        Stroke::NONE,
    ));
}

/// The count a card is about, with its limit field below.
fn big_number(painter: &egui::Painter, card: Rect, value: &str, limit: &str) {
    let center_x = card.center().x;
    painter.text(
        pos2(center_x, card.bottom() - 52.0),
        Align2::CENTER_CENTER,
        value,
        theme::number(text::NUMBER),
        color::TEXT_STRONG,
    );
    let field = Rect::from_center_size(pos2(center_x, card.bottom() - 24.0), vec2(76.0, 20.0));
    painter.rect_filled(field, CornerRadius::same(radius::SMALL), color::INSET);
    let ink = if limit.parse::<f64>().is_ok() {
        color::TEXT
    } else {
        color::TEXT_FAINT
    };
    painter.text(
        field.center(),
        Align2::CENTER_CENTER,
        limit,
        FontId::proportional(text::SMALL),
        ink,
    );
}
