//! M2: the graph model, linking rules, validation and save files.

mod common;

use aic_core::{FileError, Graph, Issue, LinkError, LoadedGraph, NodeKind, PortRef, Pos, Side};
use aic_data::Catalog;
use common::*;

fn catalog() -> Catalog {
    TestCatalog::new()
        .facility("furnace", 5.0)
        .facility("pump", 10.0)
        .facility_with("wuling_only", 1.0, 10.0, &[JINLONG])
        .recipe("ingot", "furnace", 2.0, &[("ore", 1.0)], &[("ingot", 1.0)])
        .recipe(
            "plate",
            "furnace",
            2.0,
            &[("ingot", 2.0)],
            &[("plate", 1.0)],
        )
        .recipe("water", "pump", 1.0, &[], &[("liquid_water", 1.0)])
        .recipe_with(
            "wuling_recipe",
            &["furnace"],
            2.0,
            &[("ore", 1.0)],
            &[("ingot", 1.0)],
            None,
            &[JINLONG],
        )
        .recipe(
            "wuling_facility",
            "wuling_only",
            2.0,
            &[("ore", 1.0)],
            &[("ingot", 1.0)],
        )
        .build()
}

fn graph() -> Graph {
    Graph::new(region(TUNDRA))
}

#[test]
fn recipe_ports_follow_the_recipe() {
    let catalog = catalog();
    let mut g = graph();
    let ingot = add_recipe(&mut g, &catalog, "ingot", None);
    let plate = add_recipe(&mut g, &catalog, "plate", None);
    let link = link(&mut g, &catalog, ingot, plate, "ingot");
    let stored = g.link(link).unwrap();
    assert_eq!(
        (stored.from, stored.to, &stored.item),
        (ingot, plate, &item("ingot"))
    );
}

#[test]
fn linking_a_port_the_node_lacks_is_refused() {
    let catalog = catalog();
    let mut g = graph();
    let ingot = add_recipe(&mut g, &catalog, "ingot", None);
    let plate = add_recipe(&mut g, &catalog, "plate", None);
    // `ore` is an input of `ingot`, not an output.
    let error = g
        .connect(
            &catalog,
            PortRef::new(ingot, "ore"),
            PortRef::new(plate, "ingot"),
        )
        .unwrap_err();
    assert_eq!(
        error,
        LinkError::NoSuchPort {
            node: ingot,
            side: Side::Output,
            item: Some(item("ore"))
        }
    );
}

#[test]
fn mismatched_items_are_refused() {
    let catalog = catalog();
    let mut g = graph();
    let ingot = add_recipe(&mut g, &catalog, "ingot", None);
    let ingot2 = add_recipe(&mut g, &catalog, "ingot", None);
    let error = g
        .connect(
            &catalog,
            PortRef::new(ingot, "ingot"),
            PortRef::new(ingot2, "ore"),
        )
        .unwrap_err();
    assert_eq!(
        error,
        LinkError::ItemMismatch {
            from: item("ingot"),
            to: item("ore")
        }
    );
}

#[test]
fn solid_to_fluid_is_refused() {
    let catalog = catalog();
    let mut g = graph();
    let water = add_recipe(&mut g, &catalog, "water", None);
    let ingot = add_recipe(&mut g, &catalog, "ingot", None);
    let error = g
        .connect(
            &catalog,
            PortRef::new(water, "liquid_water"),
            PortRef::new(ingot, "ore"),
        )
        .unwrap_err();
    assert_eq!(
        error,
        LinkError::SolidFluid {
            from: item("liquid_water"),
            to: item("ore")
        }
    );
}

#[test]
fn self_links_and_duplicates_are_refused() {
    let catalog = catalog();
    let mut g = graph();
    let splitter = add(&mut g, NodeKind::Splitter { item: None });
    let ingot = add_recipe(&mut g, &catalog, "ingot", None);
    assert_eq!(
        g.connect(&catalog, PortRef::any(splitter), PortRef::any(splitter)),
        Err(LinkError::SelfLink)
    );
    link(&mut g, &catalog, ingot, splitter, "ingot");
    assert_eq!(
        g.connect(
            &catalog,
            PortRef::new(ingot, "ingot"),
            PortRef::any(splitter)
        ),
        Err(LinkError::Duplicate)
    );
}

#[test]
fn logistics_nodes_take_the_item_of_their_first_link() {
    let catalog = catalog();
    let mut g = graph();
    let ingot = add_recipe(&mut g, &catalog, "ingot", None);
    let splitter = add(&mut g, NodeKind::Splitter { item: None });
    let merger = add(&mut g, NodeKind::Merger { item: None });

    // Two unset logistics nodes have no item to agree on.
    assert_eq!(
        g.connect(&catalog, PortRef::any(splitter), PortRef::any(merger)),
        Err(LinkError::NoItem)
    );

    let first = link(&mut g, &catalog, ingot, splitter, "ingot");
    assert_eq!(g.node(splitter).unwrap().kind.item(), Some(&item("ingot")));
    // Now the splitter's item flows on to the merger.
    link(&mut g, &catalog, splitter, merger, "ingot");
    assert_eq!(g.node(merger).unwrap().kind.item(), Some(&item("ingot")));

    // A second item is refused.
    let water = add_recipe(&mut g, &catalog, "water", None);
    assert!(matches!(
        g.connect(
            &catalog,
            PortRef::new(water, "liquid_water"),
            PortRef::any(splitter)
        ),
        Err(LinkError::SolidFluid { .. })
    ));

    // Removing the last link frees the item again.
    g.remove_link(first);
    assert_eq!(g.node(splitter).unwrap().kind.item(), Some(&item("ingot")));
    let rest: Vec<_> = g.links().keys().collect();
    for id in rest {
        g.remove_link(id);
    }
    assert_eq!(g.node(splitter).unwrap().kind.item(), None);
    assert_eq!(g.node(merger).unwrap().kind.item(), None);
}

#[test]
fn sources_only_output_and_sinks_only_input() {
    let catalog = catalog();
    let mut g = graph();
    let source = add(
        &mut g,
        NodeKind::Source {
            item: item("ore"),
            limit: 60.0,
        },
    );
    let sink = add(
        &mut g,
        NodeKind::Sink {
            item: item("ore"),
            limit: None,
        },
    );
    assert!(matches!(
        g.connect(&catalog, PortRef::any(sink), PortRef::any(source)),
        Err(LinkError::NoSuchPort { .. })
    ));
    assert!(
        g.connect(&catalog, PortRef::any(source), PortRef::any(sink))
            .is_ok()
    );
}

#[test]
fn removing_a_node_removes_its_links() {
    let catalog = catalog();
    let mut g = graph();
    let ingot = add_recipe(&mut g, &catalog, "ingot", None);
    let plate = add_recipe(&mut g, &catalog, "plate", None);
    link(&mut g, &catalog, ingot, plate, "ingot");
    g.remove_node(ingot);
    assert!(g.links().is_empty());
    assert_eq!(g.nodes().len(), 1);
}

#[test]
fn limits_must_be_finite_and_not_negative() {
    let catalog = catalog();
    let mut g = graph();
    let ingot = add_recipe(&mut g, &catalog, "ingot", None);
    assert!(g.set_limit(ingot, Some(2.5)).is_ok());
    assert!(g.set_limit(ingot, Some(-1.0)).is_err());
    assert!(g.set_limit(ingot, Some(f64::NAN)).is_err());
    let source = add(
        &mut g,
        NodeKind::Source {
            item: item("ore"),
            limit: 60.0,
        },
    );
    // A source's limit is required.
    assert!(g.set_limit(source, None).is_err());
    assert!(g.set_limit(source, Some(30.0)).is_ok());
    assert!(matches!(
        g.node(source).unwrap().kind,
        NodeKind::Source { limit: 30.0, .. }
    ));
}

#[test]
fn a_valid_graph_has_no_issues() {
    let catalog = catalog();
    let mut g = graph();
    let ingot = add_recipe(&mut g, &catalog, "ingot", Some(1.0));
    let plate = add_recipe(&mut g, &catalog, "plate", None);
    link(&mut g, &catalog, ingot, plate, "ingot");
    assert_eq!(g.validate(&catalog), vec![]);
}

#[test]
fn recipes_and_facilities_outside_the_region_are_issues() {
    let catalog = catalog();
    let mut g = graph();
    let recipe = add_recipe(&mut g, &catalog, "wuling_recipe", None);
    let facility = add_recipe(&mut g, &catalog, "wuling_facility", None);
    assert_eq!(
        g.validate(&catalog),
        vec![
            Issue::RecipeOutsideRegion {
                node: recipe,
                recipe: "wuling_recipe".into()
            },
            Issue::FacilityOutsideRegion {
                node: facility,
                facility: "wuling_only".into()
            },
        ]
    );
}

#[test]
fn a_facility_that_cannot_run_the_recipe_is_an_issue() {
    let catalog = catalog();
    let mut g = graph();
    let node = g.add_node(
        NodeKind::Recipe {
            recipe: "ingot".into(),
            facility: "pump".into(),
            limit: None,
        },
        Pos::default(),
    );
    assert_eq!(
        g.validate(&catalog),
        vec![Issue::FacilityCannotRun {
            node,
            facility: "pump".into()
        }]
    );
}

fn sample(catalog: &Catalog) -> Graph {
    let mut g = graph();
    let source = g.add_node(
        NodeKind::Source {
            item: item("ore"),
            limit: 60.0,
        },
        Pos { x: 10.0, y: -20.5 },
    );
    let ingot = add_recipe(&mut g, catalog, "ingot", Some(1.5));
    let splitter = add(&mut g, NodeKind::Splitter { item: None });
    let storage = add(&mut g, NodeKind::Storage { item: None });
    let port = add(
        &mut g,
        NodeKind::ControlPort {
            item: None,
            limit: 12.0,
        },
    );
    let sink = add(
        &mut g,
        NodeKind::Sink {
            item: item("ingot"),
            limit: Some(5.0),
        },
    );
    let merger = add(&mut g, NodeKind::Merger { item: None });
    link(&mut g, catalog, source, ingot, "ore");
    link(&mut g, catalog, ingot, splitter, "ingot");
    link(&mut g, catalog, splitter, storage, "ingot");
    link(&mut g, catalog, splitter, port, "ingot");
    link(&mut g, catalog, port, merger, "ingot");
    link(&mut g, catalog, merger, sink, "ingot");
    g
}

#[test]
fn save_and_load_round_trip() {
    let catalog = catalog();
    let original = sample(&catalog);
    let json = original.to_json("v1.5.3");
    let LoadedGraph {
        graph,
        data_version,
        ..
    } = Graph::from_json(&catalog, &json).unwrap();
    assert_eq!(data_version, "v1.5.3");
    assert_eq!(graph.region(), &region(TUNDRA));
    assert_eq!(graph.nodes().len(), 7);
    assert_eq!(graph.links().len(), 6);
    assert_eq!(graph.to_json("v1.5.3"), json);
    assert!(
        graph
            .nodes()
            .values()
            .any(|n| n.pos == Pos { x: 10.0, y: -20.5 })
    );
}

#[test]
fn save_file_uses_ids_and_a_format_version() {
    let catalog = catalog();
    let value: serde_json::Value =
        serde_json::from_str(&sample(&catalog).to_json("v1.5.3")).unwrap();
    assert_eq!(value["format_version"], 1);
    assert_eq!(value["data_version"], "v1.5.3");
    assert_eq!(value["region"], TUNDRA);
    let recipe = value["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["kind"] == "recipe")
        .unwrap();
    assert_eq!(recipe["recipe"], "ingot");
    assert_eq!(recipe["facility"], "furnace");
    assert_eq!(recipe["limit"], 1.5);
}

#[test]
fn unknown_recipe_on_load_becomes_a_missing_recipe_node() {
    let catalog = catalog();
    let json = sample(&catalog)
        .to_json("v1.5.3")
        .replace("\"recipe\": \"ingot\"", "\"recipe\": \"gone\"");
    let LoadedGraph { graph, .. } = Graph::from_json(&catalog, &json).unwrap();
    let missing = graph
        .nodes()
        .values()
        .find(|n| matches!(n.kind, NodeKind::MissingRecipe { .. }))
        .expect("the unknown recipe is kept");
    assert!(
        matches!(&missing.kind, NodeKind::MissingRecipe { recipe, limit: Some(1.5), .. } if recipe.as_str() == "gone")
    );
    // Its links survive, and saving writes it back unchanged.
    assert_eq!(graph.links().len(), 6);
    assert!(graph.to_json("v1.5.3").contains("\"recipe\": \"gone\""));
    assert!(
        graph
            .validate(&catalog)
            .iter()
            .any(|i| matches!(i, Issue::MissingRecipe { .. }))
    );
}

#[test]
fn newer_format_versions_are_refused() {
    let catalog = catalog();
    let json = sample(&catalog)
        .to_json("v1.5.3")
        .replace("\"format_version\": 1", "\"format_version\": 99");
    assert!(matches!(
        Graph::from_json(&catalog, &json),
        Err(FileError::UnsupportedFormat { found: 99, .. })
    ));
}

#[test]
fn links_to_unknown_nodes_are_refused() {
    let catalog = catalog();
    let mut value: serde_json::Value =
        serde_json::from_str(&sample(&catalog).to_json("v1.5.3")).unwrap();
    value["links"][0]["to"] = serde_json::json!(999);
    let error = Graph::from_json(&catalog, &value.to_string()).unwrap_err();
    assert!(
        matches!(error, FileError::UnknownNode { id: 999, .. }),
        "{error:?}"
    );
}

#[test]
fn malformed_files_name_the_json_path() {
    let catalog = catalog();
    let json = sample(&catalog)
        .to_json("v1.5.3")
        .replace("\"limit\": 60.0", "\"limit\": \"lots\"");
    let FileError::Json { path, .. } = Graph::from_json(&catalog, &json).unwrap_err() else {
        panic!("expected a JSON error");
    };
    assert!(path.starts_with("nodes[0]"), "{path}");
}

#[test]
fn validation_catches_what_a_hand_edited_file_can_break() {
    let catalog = catalog();
    let mut value: serde_json::Value =
        serde_json::from_str(&sample(&catalog).to_json("v1.5.3")).unwrap();
    // Point the source → ingot link at an item the ingot recipe doesn't take.
    value["links"][0]["item"] = serde_json::json!("plate");
    let LoadedGraph { graph, .. } = Graph::from_json(&catalog, &value.to_string()).unwrap();
    let issues = graph.validate(&catalog);
    assert!(
        issues.iter().any(|i| matches!(
            i,
            Issue::BadLink {
                error: LinkError::NoSuchPort { .. },
                ..
            }
        )),
        "{issues:?}"
    );
}

#[test]
fn rebinding_to_other_data_marks_missing_recipes_and_restores_known_ones() {
    let old = TestCatalog::new()
        .facility("furnace", 5.0)
        .recipe("ingot", "furnace", 2.0, &[("ore", 1.0)], &[("ingot", 1.0)])
        .build();
    let new = TestCatalog::new()
        .facility("furnace", 5.0)
        .recipe(
            "plate",
            "furnace",
            2.0,
            &[("ingot", 1.0)],
            &[("plate", 1.0)],
        )
        .build();
    let mut g = Graph::new(region(TUNDRA));
    let ingot = add_recipe(&mut g, &old, "ingot", Some(2.0));

    g.rebind(&new);
    assert_eq!(
        g.node(ingot).unwrap().kind,
        NodeKind::MissingRecipe {
            recipe: "ingot".into(),
            facility: "furnace".into(),
            limit: Some(2.0),
        }
    );
    g.rebind(&old);
    assert!(matches!(
        g.node(ingot).unwrap().kind,
        NodeKind::Recipe { .. }
    ));
}

#[test]
fn the_canvas_view_is_saved_with_the_graph() {
    let catalog = catalog();
    let g = Graph::new(region(TUNDRA));
    let view = aic_core::CanvasView {
        x: -120.0,
        y: 40.0,
        zoom: 0.5,
    };
    let loaded = Graph::from_json(&catalog, &g.to_json_with_view("test", Some(view))).unwrap();
    assert_eq!(loaded.view, Some(view));
    assert_eq!(
        Graph::from_json(&catalog, &g.to_json("test")).unwrap().view,
        None
    );
}

#[test]
fn only_an_empty_graph_changes_region() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    g.set_region(region(JINLONG)).unwrap();
    assert_eq!(g.region(), &region(JINLONG));
    add_recipe(&mut g, &catalog, "ingot", None);
    assert_eq!(
        g.set_region(region(TUNDRA)),
        Err(aic_core::EditError::NotEmpty)
    );
}
