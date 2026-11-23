//! The tutorial: when it opens by itself and what it teaches. The app draws it.

use crate::{Command, Session};

/// The commands the Keys row teaches, two to a line.
pub const KEYS: [&[Command]; 3] = [
    &[Command::Copy, Command::Paste],
    &[Command::Undo, Command::Redo],
    &[Command::Delete],
];

/// The command the save row shows the keys of.
pub const SAVE: Command = Command::Save;

/// Whether the tutorial opens once the app has started: when the start reopened neither a file
/// nor a draft, so the player faces an empty Untitled graph. The first run is one of those.
pub fn opens_at_start(session: &Session) -> bool {
    session.document().path().is_none()
}

/// The Fluent key of the line on how the graph is kept: on disk, or in the browser's storage.
pub fn saving_key(session: &Session) -> &'static str {
    if session.is_browser() {
        "tutorial-save-browser"
    } else {
        "tutorial-save-desktop"
    }
}

#[cfg(test)]
mod tests {
    use aic_core::{NodeKind, Pos};

    use super::*;
    use crate::settings::Availability;
    use crate::store::BrowserFiles;
    use crate::testing::fixture_catalog;

    #[test]
    fn an_untitled_graph_opens_the_tutorial_and_a_saved_one_does_not() {
        let mut session = Session::new(fixture_catalog(), Availability::default());
        assert!(opens_at_start(&session));
        session.use_browser_files(BrowserFiles::default());
        let smelter = NodeKind::Recipe {
            recipe: "iron_ingot".into(),
            facility: "smelter".into(),
            limit: Some(1.0),
        };
        session.add_node(smelter, Pos::default(), None);
        session.save_now(web_time::Instant::now(), None);
        assert!(
            !opens_at_start(&session),
            "a draft is a document to go back to"
        );
    }

    #[test]
    fn the_browser_explains_its_own_saving() {
        let mut session = Session::new(fixture_catalog(), Availability::default());
        assert_eq!(saving_key(&session), "tutorial-save-desktop");
        session.use_browser_files(BrowserFiles::default());
        assert_eq!(saving_key(&session), "tutorial-save-browser");
    }
}
