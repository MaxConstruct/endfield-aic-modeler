//! Typed IDs. Every ID is the game's own string ID (`copper_nugget`, `furnance_1`), never a display name.

use std::borrow::Borrow;
use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! string_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(id: &str) -> Self {
                Self(id.to_owned())
            }
        }

        impl From<String> for $name {
            fn from(id: String) -> Self {
                Self(id)
            }
        }

        // Lets maps keyed by this ID be looked up with a plain `&str`. The derived `Hash` hashes only
        // the inner `String`, so it agrees with `str`'s hash as `Borrow` requires.
        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

string_id!(
    /// An item: anything that moves on a belt or pipe, including facilities as buildable items.
    ItemId
);
string_id!(
    /// A facility: a building type that performs recipes.
    FacilityId
);
string_id!(
    /// A recipe.
    RecipeId
);
string_id!(
    /// A region: Valley IV (`tundra`) or Wuling (`jinlong`).
    RegionId
);
string_id!(
    /// An item or recipe category, used to filter the recipe browser.
    CategoryId
);
