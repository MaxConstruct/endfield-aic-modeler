//! The recipe browser: every visible recipe of the graph's region, searchable by English name and
//! filterable by category. A click adds the recipe's node to the middle of the canvas.

use aic_data::{Catalog, CategoryId, RecipeId, RegionId};
use eframe::egui::{self, Ui};

use crate::i18n::Strings;
use crate::picker::{self, Matching, Origin};
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
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        catalog: &Catalog,
        strings: &Strings,
        region: &RegionId,
    ) -> Option<RecipeId> {
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
                for (id, category) in catalog.categories() {
                    ui.selectable_value(&mut self.category, Some(id.clone()), &category.name);
                }
            });
        ui.separator();
        let by_name = Matching {
            name: true,
            inputs: false,
            outputs: false,
        };
        let mut clicked = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            // One line per recipe; a long name is cut short and shown whole on hover.
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            let recipes = picker::recipes(catalog, region, &Origin::Canvas, &self.query, by_name);
            for recipe in recipes {
                if self
                    .category
                    .as_ref()
                    .is_some_and(|c| *c != recipe.category)
                {
                    continue;
                }
                let image = catalog
                    .recipe_image(recipe.id.as_str())
                    .and_then(|(kind, id)| widgets::image_atom(ui.ctx(), catalog, kind, id, ICON));
                let button = match image {
                    Some(image) => egui::Button::new((image, recipe.name.as_str())),
                    None => egui::Button::new(recipe.name.as_str()),
                }
                .min_size(egui::vec2(ui.available_width(), 0.0));
                if ui
                    .add(button)
                    .on_hover_text(strings.get("browser-add-hint"))
                    .clicked()
                {
                    clicked = Some(recipe.id.clone());
                }
            }
        });
        clicked
    }
}
