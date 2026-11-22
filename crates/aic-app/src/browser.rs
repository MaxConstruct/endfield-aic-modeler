//! The recipe browser: every visible recipe of every region, searchable by English name and
//! filterable by category. A recipe only some regions have carries their names as badges. A click
//! adds the recipe's node to the middle of the canvas.

use aic_data::{Catalog, CategoryId, RecipeId};
use eframe::egui::{self, Ui};

use crate::i18n::Strings;
use crate::widgets;

/// Size of the icon beside each recipe.
const ICON: f32 = 32.0;

#[derive(Default)]
pub struct Browser {
    query: String,
    category: Option<CategoryId>,
}

impl Browser {
    /// Draws the browser and returns the recipe the player clicked.
    pub fn ui(&mut self, ui: &mut Ui, catalog: &Catalog, strings: &Strings) -> Option<RecipeId> {
        ui.heading(strings.get("browser-title"));
        ui.add(
            egui::TextEdit::singleline(&mut self.query)
                .hint_text(strings.get("browser-search-hint"))
                .desired_width(f32::INFINITY),
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
            let recipes =
                aic_editor::browser::recipes(catalog, &self.query, self.category.as_ref());
            for recipe in recipes {
                let image = catalog
                    .recipe_image(recipe.id.as_str(), crate::icons::has)
                    .and_then(|(kind, id)| widgets::image_atom(ui.ctx(), kind, id, ICON));
                // The region badges go between the icon and the name, painted once laid out.
                let badges = widgets::RegionBadges::new(ui, catalog, recipe, 1.0);
                let badge_id = egui::Id::new(("browser-badges", recipe.id.as_str()));
                let mut atoms = egui::Atoms::default();
                if let Some(image) = image {
                    atoms.push_right(image);
                }
                if !badges.is_empty() {
                    atoms.push_right(egui::Atom::custom(badge_id, badges.size()));
                }
                atoms.push_right(recipe.name.as_str());
                let button =
                    egui::Button::new(atoms).min_size(egui::vec2(ui.available_width(), 0.0));
                let laid_out = button.atom_ui(ui);
                if let Some(rect) = laid_out.rect(badge_id) {
                    badges.paint(ui.painter(), rect.left_center());
                }
                if laid_out.response.on_hover_text(hint.as_str()).clicked() {
                    clicked = Some(recipe.id.clone());
                }
            }
        });
        clicked
    }
}
