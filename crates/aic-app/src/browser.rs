//! The recipe browser: every visible recipe of every region, searchable by English name and
//! filterable by category. A recipe the graph's region lacks is muted, with the badges of the
//! regions that have it. A click adds the recipe's node to the middle of the canvas.

use aic_data::{Catalog, CategoryId, RecipeId, RegionId};
use eframe::egui::{self, AtomExt as _, Ui};

use crate::i18n::Strings;
use crate::{theme, widgets};
use aic_editor::names;

/// Size of the icon beside each recipe.
const ICON: f32 = 32.0;

#[derive(Default)]
pub struct Browser {
    query: String,
    category: Option<CategoryId>,
}

impl Browser {
    /// Draws the browser and returns the recipe the player clicked.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        catalog: &Catalog,
        strings: &Strings,
        graph_region: &RegionId,
    ) -> Option<RecipeId> {
        ui.heading(strings.get("browser-title"));
        ui.add(
            widgets::wide_field(ui, &mut self.query).hint_text(strings.get("browser-search-hint")),
        );
        let all = strings.get("browser-all-categories");
        let selected = self
            .category
            .as_ref()
            .and_then(|id| catalog.categories().get(id))
            .map_or(all.clone(), |category| category.name.clone());
        egui::ComboBox::from_id_salt("browser-category")
            .selected_text(selected)
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.category, None, all);
                for category in aic_editor::browser::categories(catalog) {
                    ui.selectable_value(
                        &mut self.category,
                        Some(category.id.clone()),
                        &category.name,
                    );
                }
            });
        ui.separator();
        let mut clicked = None;
        let hint = strings.get("browser-add-hint");
        egui::ScrollArea::vertical().show(ui, |ui| {
            // One line per recipe; a long name is cut short and shown whole on hover.
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            // A name is cut short to the room its row leaves it. egui lays text out at a whole
            // number of points (epaint rounds every wrap width to one), so a name left 124.5 points
            // takes 125 and its row comes out wider than the list. That row widened the list for
            // the rows after it and the panel for the next frame: the panel crept wider for about
            // ten frames after start-up. The name's room is whole when everything else in its row
            // is: the list here, the icon's slot (a third of the icons aren't square, and keep
            // their shape inside it) and the badges' slot below; the margins and gaps are whole.
            ui.set_max_width(ui.available_width().floor());
            let recipes =
                aic_editor::browser::recipes(catalog, &self.query, self.category.as_ref());
            for recipe in recipes {
                let image = catalog
                    .recipe_image(recipe.id.as_str(), crate::icons::has)
                    .and_then(|(kind, id)| widgets::image_atom(ui.ctx(), kind, id, ICON));
                // The region badges go at the row's right end, painted once laid out.
                let badges = widgets::RegionBadges::new(ui, catalog, graph_region, recipe, 1.0);
                let badge_id = egui::Id::new(("browser-badges", recipe.id.as_str()));
                let mut atoms = egui::Atoms::default();
                if let Some(image) = image {
                    // A square slot, whatever the icon's shape.
                    atoms.push_right(egui::Atom::from(image).atom_size(egui::Vec2::splat(ICON)));
                }
                if badges.is_empty() {
                    atoms.push_right(recipe.name.as_str());
                } else {
                    let name = egui::RichText::new(&recipe.name);
                    atoms.push_right(name.color(theme::color::TEXT_MUTED));
                    atoms.push_right(egui::Atom::grow());
                    let size = badges.size();
                    let slot = egui::vec2(size.x.ceil(), size.y);
                    atoms.push_right(egui::Atom::custom(badge_id, slot));
                }
                let button =
                    egui::Button::new(atoms).min_size(egui::vec2(ui.available_width(), 0.0));
                let laid_out = button.atom_ui(ui);
                if let Some(rect) = laid_out.rect(badge_id) {
                    badges.paint(ui.painter(), rect.left_center());
                }
                let response = laid_out.response.on_hover_ui(|ui| {
                    ui.label(&recipe.name);
                    if let Some(regions) = names::recipe_regions(catalog, strings, recipe) {
                        ui.label(regions);
                    }
                    ui.weak(hint.as_str());
                });
                if response.clicked() {
                    clicked = Some(recipe.id.clone());
                }
            }
        });
        clicked
    }
}
