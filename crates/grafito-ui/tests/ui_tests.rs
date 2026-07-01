//! Tests de integración para los módulos públicos de `grafito-ui`.
//!
//! Cubren la paleta de comandos (búsqueda/filtrado), el sistema de temas
//! (DARK/LIGHT) y el enum `Tool` (valor por defecto).

use grafito_ui::command_palette::{all_commands, CommandPaletteState};
use grafito_ui::theme::{DARK, LIGHT};
use grafito_ui::toolbar::ToolGroupId;
use grafito_ui::Tool;

// ── Command palette ──────────────────────────────────────────────────────

#[test]
fn command_palette_empty_search_returns_all_commands() {
    let state = CommandPaletteState::default();
    let filtered = state.filtered_commands();
    assert_eq!(filtered.len(), all_commands().len());
}

#[test]
fn command_palette_fuzzy_match_narrows_results() {
    let state = CommandPaletteState {
        search: "ellipse".to_string(),
        ..Default::default()
    };
    let filtered = state.filtered_commands();
    assert!(!filtered.is_empty(), "searching 'ellipse' should match");
    // Every filtered command name should contain "ellipse" (case-insensitive)
    // or its category should.
    for cmd in &filtered {
        let matches = cmd.name.to_lowercase().contains("ellipse")
            || cmd.category.to_lowercase().contains("ellipse");
        assert!(
            matches,
            "filtered command '{}' should match 'ellipse'",
            cmd.name
        );
    }
}

#[test]
fn command_palette_gibberish_search_returns_empty() {
    let state = CommandPaletteState {
        search: "zzzqqqx".to_string(),
        ..Default::default()
    };
    let filtered = state.filtered_commands();
    assert!(
        filtered.is_empty(),
        "gibberish search should return no commands"
    );
}

#[test]
fn command_palette_search_is_case_insensitive() {
    let state = CommandPaletteState {
        search: "DERIVATIVE".to_string(),
        ..Default::default()
    };
    let filtered = state.filtered_commands();
    assert!(filtered.iter().any(|c| c.name == "Derivative"));
}

#[test]
fn command_palette_state_defaults_to_closed() {
    let state = CommandPaletteState::default();
    assert!(!state.open);
    assert!(state.search.is_empty());
    assert_eq!(state.selected_index, 0);
}

// ── Theme ────────────────────────────────────────────────────────────────

#[test]
fn dark_and_light_themes_have_distinct_canvas_colors() {
    assert_ne!(DARK.canvas_bg, LIGHT.canvas_bg);
}

#[test]
fn dark_and_light_themes_have_distinct_accents() {
    assert_ne!(DARK.accent, LIGHT.accent);
}

#[test]
fn dark_theme_is_actually_dark() {
    // A dark canvas has low R channel value.
    assert!(DARK.canvas_bg.r() < 50, "DARK canvas_bg should be dark");
    assert!(DARK.panel_bg.r() < 50, "DARK panel_bg should be dark");
}

#[test]
fn light_theme_is_actually_light() {
    assert!(LIGHT.canvas_bg.r() > 200, "LIGHT canvas_bg should be light");
    assert!(LIGHT.panel_bg.r() > 200, "LIGHT panel_bg should be light");
}

#[test]
fn themes_define_object_colors() {
    // Object colors are used by the algebra panel legend; they must be set.
    assert_ne!(DARK.object_point, DARK.canvas_bg);
    assert_ne!(DARK.object_line, DARK.canvas_bg);
    assert_ne!(DARK.object_function, DARK.canvas_bg);
    assert_ne!(LIGHT.object_point, LIGHT.canvas_bg);
}

// ── Toolbar / Tool ────────────────────────────────────────────────────────

#[test]
fn tool_select_is_the_default() {
    assert_eq!(Tool::default(), Tool::Select);
}

#[test]
fn tool_select_has_a_cursor_icon() {
    // Smoke test: cursor_icon must not panic for the default tool.
    let _icon = Tool::Select.cursor_icon();
}

#[test]
fn tool_enum_can_be_switched() {
    let mut tool = Tool::default();
    assert_eq!(tool, Tool::Select);
    tool = Tool::Point;
    assert_eq!(tool, Tool::Point);
    assert_ne!(tool, Tool::Select);
}

#[test]
fn point_group_does_not_expose_3d_tools() {
    let (_, tools) = ToolGroupId::Point.def();
    assert!(tools.iter().any(|(tool, _, _)| *tool == Tool::Point));
    assert!(!tools.iter().any(|(tool, _, _)| *tool == Tool::Point3D));
}

#[test]
fn three_d_and_dynamics_groups_expose_3d_tools() {
    let (_, three_d_tools) = ToolGroupId::ThreeD.def();
    assert!(three_d_tools
        .iter()
        .any(|(tool, _, _)| *tool == Tool::Plane3D));
    assert!(three_d_tools
        .iter()
        .any(|(tool, _, _)| *tool == Tool::Line3D));
    assert!(three_d_tools
        .iter()
        .any(|(tool, _, _)| *tool == Tool::Surface3D));

    let (_, dynamics_tools) = ToolGroupId::Dynamics.def();
    assert!(dynamics_tools
        .iter()
        .any(|(tool, _, _)| *tool == Tool::Attractor));
    assert!(dynamics_tools
        .iter()
        .any(|(tool, _, _)| *tool == Tool::VectorField3D));
    assert!(dynamics_tools
        .iter()
        .any(|(tool, _, _)| *tool == Tool::HyperSurface4D));
}
