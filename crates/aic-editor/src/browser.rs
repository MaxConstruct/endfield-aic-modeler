//! The recipe browser's model: every visible recipe of every region, searchable by English name and
//! filterable by category. Drawing is the UI's.

use aic_data::{Catalog, Category, CategoryId, Recipe};

use crate::picker::{self, Matching, Origin};

/// The categories to filter by, in data order.
pub fn categories(catalog: &Catalog) -> impl Iterator<Item = &Category> {
    catalog.categories().values()
}

/// The recipes the browser lists: those whose name contains `query` (case-insensitive), of
/// `category` if one is chosen, in data order.
pub fn recipes<'a>(
    catalog: &'a Catalog,
    query: &str,
    category: Option<&CategoryId>,
) -> Vec<&'a Recipe> {
    let by_name = Matching {
        name: true,
        inputs: false,
        outputs: false,
    };
    picker::recipes(catalog, &Origin::Canvas, query, by_name)
        .into_iter()
        .filter(|recipe| category.is_none_or(|c| *c == recipe.category))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::fixture_catalog;

    fn ids(recipes: Vec<&Recipe>) -> Vec<&str> {
        recipes.into_iter().map(|r| r.id.as_str()).collect()
    }

    #[test]
    fn search_and_category_narrow_the_list() {
        let catalog = fixture_catalog();
        assert_eq!(
            ids(recipes(&catalog, "plate", None)),
            ["iron_plate", "reinforced_plate", "coupon-iron_plate"]
        );
        let material = CategoryId::from("material");
        assert_eq!(
            ids(recipes(&catalog, "", Some(&material))),
            ["iron_ore", "iron_ingot"]
        );
        assert_eq!(
            ids(recipes(&catalog, "PLATE", Some(&material))),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn the_categories_come_in_data_order() {
        let catalog = fixture_catalog();
        let names: Vec<&str> = categories(&catalog).map(|c| c.id.as_str()).collect();
        assert_eq!(names, ["material", "component"]);
    }
}
