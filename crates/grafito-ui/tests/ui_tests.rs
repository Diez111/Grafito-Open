#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Tests de integración para los módulos públicos de `grafito-ui`.
//!
//! Cubren la paleta de comandos (búsqueda/filtrado), el sistema de temas
//! (DARK/LIGHT) y el enum `Tool` (valor por defecto).

use grafito_ui::command_palette::{all_commands, CommandPaletteState};
use grafito_ui::theme::{DARK, LIGHT};
use grafito_ui::toolbar::{
    icon_for_tool, ToolGroupId, ALL_GROUPS, TOOLBAR_BUTTON_SIZE, TOOLBAR_PANEL_HEIGHT,
    TOOLBAR_VERTICAL_PADDING,
};
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
    use grafito_ui::command_palette::fuzzy_match;
    let state = CommandPaletteState {
        search: "ellipse".to_string(),
        ..Default::default()
    };
    let filtered = state.filtered_commands();
    assert!(!filtered.is_empty(), "searching 'ellipse' should match");
    // Búsqueda difusa bilingüe: cada resultado debe coincidir (subcadena o
    // subsecuencia en orden) en nombre, categoría, syntax_hint, ayuda o alias.
    for cmd in &filtered {
        let matches = [
            cmd.name,
            cmd.category,
            cmd.syntax_hint,
            cmd.help,
            cmd.keywords,
        ]
        .iter()
        .any(|haystack| fuzzy_match("ellipse", haystack));
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

#[test]
fn command_palette_clamps_selection_after_filtering() {
    let mut state = CommandPaletteState {
        search: "derivative".to_string(),
        selected_index: 999,
        ..Default::default()
    };

    let filtered_len = state.filtered_commands().len();
    assert!(filtered_len > 0);
    state.clamp_selected_index();

    assert!(state.selected_index < filtered_len);
}

#[test]
fn command_palette_templates_skip_actions_and_fix_visual_names() {
    let commands = all_commands();
    let thomas = commands
        .iter()
        .find(|cmd| cmd.name == "Thomas (Butterfly)")
        .expect("Thomas command should be present");
    assert_eq!(thomas.input_template().as_deref(), Some("Thomas["));

    let save = commands
        .iter()
        .find(|cmd| cmd.name == "Guardar")
        .expect("Guardar command should be present");
    assert_eq!(save.selection_key, "Save");
    assert!(save.input_template().is_none());

    let derivative = commands
        .iter()
        .find(|cmd| cmd.name == "Derivative")
        .expect("Derivative command should be present");
    assert_eq!(derivative.input_template().as_deref(), Some("Derivative["));
}

#[test]
fn command_palette_footer_hints_navigation_keys() {
    let palette_source = include_str!("../src/command_palette.rs");
    assert!(palette_source.contains("↑↓"));
    assert!(palette_source.contains("Enter"));
    assert!(palette_source.contains("Esc"));
}

#[test]
fn command_palette_projects_registered_metadata_and_keeps_actions_explicit() {
    let commands = all_commands();
    let registered_count = commands
        .iter()
        .filter(|command| command.is_registered())
        .count();
    assert!(
        registered_count > 50,
        "stable commands must come from the registry"
    );

    for command in commands.iter().filter(|command| command.is_registered()) {
        let spec = command
            .registered_spec()
            .expect("registered palette entry must resolve to a command spec");
        assert_eq!(command.category, spec.category);
        assert_eq!(command.syntax_hint, spec.signatures[0].syntax);
        assert_eq!(command.input_template().as_deref(), Some(spec.insertion));
    }

    for action in commands.iter().filter(|command| !command.is_registered()) {
        assert!(
            action.input_template().is_none(),
            "non-command action '{}' must not insert CAS text",
            action.name
        );
    }
}

#[test]
fn stable_palette_does_not_expose_unavailable_placeholder_features() {
    let commands = all_commands();

    for unavailable in ["Button", "Image"] {
        assert!(
            commands.iter().all(|command| command.name != unavailable),
            "{unavailable} must stay out of the stable command palette"
        );
    }
    assert!(
        commands
            .iter()
            .any(|command| command.name == "SampledGraph"),
        "the static function sampler needs an honest command name"
    );
    assert!(
        commands.iter().any(|command| command.name == "Locus"),
        "the persistent dynamic locus needs a palette entry"
    );
}

#[test]
fn command_palette_fits_a_narrow_viewport() {
    assert_eq!(
        grafito_ui::command_palette::palette_window_width(317.0),
        301.0
    );
    assert_eq!(grafito_ui::command_palette::palette_window_width(0.0), 1.0);
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

#[test]
fn custom_color_controls_and_toasts_expose_accessibility_metadata() {
    let picker_source = include_str!("../src/color_picker.rs");
    let toast_source = include_str!("../src/toast.rs");

    assert!(picker_source.contains("Tono y saturación"));
    assert!(picker_source.contains("Brillo del color"));
    assert!(picker_source.contains("Opacidad del color"));
    assert!(picker_source.contains("draw_checkerboard"));
    assert!(picker_source.contains("Restaurar color original"));
    assert!(picker_source.contains("Color favorito"));
    assert!(!picker_source.contains("Ajuste preciso:"));
    assert!(!picker_source.contains("precision_drag_value"));
    assert!(!picker_source.contains("egui::WidgetType::DragValue"));
    assert!(picker_source.matches("widget_info").count() >= 4);
    assert!(toast_source.contains("WidgetInfo::labeled"));
    assert!(toast_source.contains("WidgetType::Label"));
}

#[test]
fn color_picker_reports_favorite_saves_separately_from_transient_color_edits() {
    let picker_source = include_str!("../src/color_picker.rs");

    assert!(picker_source.contains("pub struct ColorPickerOutcome"));
    assert!(picker_source.contains("color_changed"));
    assert!(!picker_source.contains("object_color_changed"));
    assert!(picker_source.contains("favorites_changed"));
    assert!(picker_source.contains(
        "pub fn show(&mut self, ui: &mut Ui, favorites: &mut [Color; 5]) -> ColorPickerOutcome"
    ));
    assert!(!picker_source
        .contains("pub fn show(&mut self, ui: &mut Ui, favorites: &mut [Color; 5]) -> bool"));
}

#[test]
fn assistant_pending_indicator_uses_the_native_thinking_orb() {
    let assistant_source = include_str!("../src/assistant.rs");

    assert!(assistant_source.contains("ThinkingOrb::new"));
    assert!(assistant_source.contains("ThinkingOrbState::Solving"));
    assert!(assistant_source.contains("ThinkingOrbState::Cancelling"));
    assert!(assistant_source.contains("Consulta remota autorizada..."));
    assert!(assistant_source.contains("Cancelando..."));
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
    assert!(!dynamics_tools
        .iter()
        .any(|(tool, _, _)| *tool == Tool::VectorField3D));
    assert!(!dynamics_tools
        .iter()
        .any(|(tool, _, _)| *tool == Tool::HyperSurface4D));
}

#[test]
fn four_d_group_exposes_centered_projected_tools_with_vector_icons() {
    let (_, tools) = ToolGroupId::FourD.def();

    assert_eq!(ToolGroupId::FourD.label(), "4D proyectado");
    assert_eq!(tools.len(), 2);
    assert!(tools.iter().any(|(tool, label, _)| {
        *tool == Tool::Tesseract4D && label.contains("centrado") && label.contains("proyectado")
    }));
    assert!(tools.iter().any(|(tool, label, _)| {
        *tool == Tool::Hypercube5D && label.contains("centrado") && label.contains("proyectado")
    }));
    assert_eq!(
        Tool::Tesseract4D.cursor_icon(),
        egui::CursorIcon::PointingHand
    );
    assert_eq!(
        Tool::Hypercube5D.cursor_icon(),
        egui::CursorIcon::PointingHand
    );
    assert_ne!(
        icon_for_tool(Tool::Tesseract4D) as usize,
        icon_for_tool(Tool::Hypercube5D) as usize,
        "the two projected-polytope actions need distinguishable vector icons"
    );
}

#[test]
fn classic_toolbar_does_not_include_dynamics_group() {
    assert!(!ALL_GROUPS.contains(&ToolGroupId::Dynamics));
}

#[test]
fn stable_toolbar_groups_do_not_expose_unavailable_placeholder_tools() {
    let groups = [
        ToolGroupId::Move,
        ToolGroupId::Point,
        ToolGroupId::Line,
        ToolGroupId::Circle,
        ToolGroupId::Polygon,
        ToolGroupId::Pencil,
        ToolGroupId::Eraser,
        ToolGroupId::Conic,
        ToolGroupId::Curve,
        ToolGroupId::Measure,
        ToolGroupId::Analysis,
        ToolGroupId::Constraint,
        ToolGroupId::Boolean,
        ToolGroupId::ThreeD,
        ToolGroupId::FourD,
        ToolGroupId::Advanced,
        ToolGroupId::Dynamics,
    ];

    for group in groups {
        let (_, tools) = group.def();
        for unavailable in [Tool::Button, Tool::Image] {
            assert!(
                tools.iter().all(|(tool, _, _)| *tool != unavailable),
                "{unavailable:?} must stay out of stable toolbar group {group:?}"
            );
        }
    }
    let (_, curve_tools) = ToolGroupId::Curve.def();
    assert!(
        curve_tools.iter().any(|(tool, _, _)| *tool == Tool::Locus),
        "Locus vive solo en GROUP_CURVE canónico (dedup: un Tool un grupo), no en Constraint"
    );
}

#[test]
fn compatibility_keeps_unavailable_tool_variants_addressable() {
    assert_eq!(Tool::Button.name(), "Button");
    assert_eq!(Tool::Image.name(), "Image");
    assert_eq!(Tool::Locus.name(), "Locus");
}

#[test]
fn toolbar_exposes_subtools_with_a_normal_click_menu() {
    let toolbar_source = include_str!("../src/toolbar.rs");
    assert!(toolbar_source.contains("show_tool_group_menu("));
    assert!(toolbar_source.contains(".constrain_to(ui.ctx().screen_rect())"));
    assert!(toolbar_source.contains("draw_group_menu_indicator"));
    assert!(toolbar_source.contains("TOOL_MENU_PREFERRED_WIDTH: f32 = 220.0"));
    assert!(toolbar_source.contains("TOOL_MENU_ITEM_HEIGHT: f32 = 30.0"));
    assert!(toolbar_source.contains("ui.set_min_width(menu_width)"));
    assert!(toolbar_source.contains(".add_sized("));
    assert!(toolbar_source.contains("ScrollArea::vertical()"));
    assert!(toolbar_source.contains(".truncate()"));
    assert!(toolbar_source.contains("memory.close_popup()"));
}

#[test]
fn compact_toolbar_has_an_explicit_overflow_route_instead_of_only_scroll() {
    let toolbar_source = include_str!("../src/toolbar.rs");

    assert!(
        toolbar_source.contains("COMPACT_TOOLBAR_MAX_WIDTH: f32 = 1_360.0")
            || toolbar_source.contains("COMPACT_TOOLBAR_MAX_WIDTH: f32 = BREAKPOINT_COMPACT")
    );
    assert!(toolbar_source.contains("compact_toolbar_overflow"));
    assert!(toolbar_source.contains("Más herramientas"));
    assert!(toolbar_source.contains("ToolGroupId::label"));
}

#[test]
fn toolbar_uses_one_fixed_height_row_without_a_scrollbar_or_nested_rows() {
    let toolbar_source = include_str!("../src/toolbar.rs");

    assert_eq!(
        TOOLBAR_PANEL_HEIGHT,
        TOOLBAR_BUTTON_SIZE + 2.0 * TOOLBAR_VERTICAL_PADDING,
        "the host panel must reserve exactly one complete button row"
    );
    assert!(
        !toolbar_source.contains("ScrollArea::horizontal()"),
        "a horizontal scrollbar cannot fit inside the fixed toolbar row"
    );

    let tool_group_start = toolbar_source
        .find("fn tool_group")
        .expect("toolbar group renderer");
    let tool_group_end = toolbar_source[tool_group_start..]
        .find("fn show_tool_group_menu")
        .map(|offset| tool_group_start + offset)
        .expect("toolbar group menu renderer");
    let tool_group_source = &toolbar_source[tool_group_start..tool_group_end];
    assert!(
        !tool_group_source.contains("ui.horizontal(|ui|"),
        "each group must allocate directly in the shared toolbar row"
    );
}

#[test]
fn subtools_have_specific_toolbar_icons() {
    let line_icon = icon_for_tool(Tool::Line) as usize;
    assert_ne!(icon_for_tool(Tool::Segment) as usize, line_icon);
    assert_ne!(icon_for_tool(Tool::Ray) as usize, line_icon);
    assert_ne!(icon_for_tool(Tool::Vector) as usize, line_icon);
    assert_ne!(icon_for_tool(Tool::Perpendicular) as usize, line_icon);
    assert_ne!(
        icon_for_tool(Tool::Tangent) as usize,
        icon_for_tool(Tool::Circle) as usize
    );
}

#[test]
fn compact_app_actions_use_vector_icon_buttons() {
    let algebra_source = include_str!("../../grafito-app/src/algebra.rs");
    let panels_source = include_str!("../../grafito-app/src/panels.rs");
    let ui_source = include_str!("../../grafito-app/src/ui.rs");

    assert!(algebra_source.contains("action_icon_button("));
    assert!(panels_source.contains("action_icon_button("));
    assert!(ui_source.contains("action_icon_button("));
    assert!(!algebra_source.contains("menu_button(\"Cfg\""));
    assert!(!algebra_source.contains("RichText::new(icon_str)"));
    assert!(!panels_source.contains(".button(if app.trig_animating"));
    assert!(!panels_source.contains("Button::new(\"Go\")"));
    assert!(!ui_source.contains("RichText::new(\"Go\")"));
}

#[test]
fn native_chrome_uses_short_state_animations() {
    let toolbar_source = include_str!("../src/toolbar.rs");
    let ui_source = include_str!("../../grafito-app/src/ui.rs");

    assert!(toolbar_source.contains("animate_bool"));
    assert!(ui_source.contains("animate_bool"));
}

#[test]
fn compact_chrome_keeps_panel_navigation_and_theme_owned_slider_colors() {
    let ui_source = include_str!("../../grafito-app/src/ui.rs");
    let algebra_source = include_str!("../../grafito-app/src/algebra.rs");

    assert!(ui_source.contains("ui.menu_button(\"Paneles\""));
    assert!(ui_source.contains("ui.menu_button(\"Más\""));
    assert!(ui_source.contains("top_chrome_uses_overflow"));
    assert!(algebra_source.contains("visuals.selection.bg_fill = theme.accent"));
    assert!(!algebra_source.contains("Color32::from_rgb(48, 52, 62)"));
}

#[test]
fn assistant_is_a_permanent_docked_panel_without_a_launcher() {
    let assistant_source = include_str!("../src/assistant.rs");
    let app_source = include_str!("../../grafito-app/src/app.rs");
    let icons_source = include_str!("../src/icons.rs");

    assert!(assistant_source.contains("SidePanel::right"));
    assert!(!assistant_source.contains("assistant_affordance"));
    assert!(!assistant_source.contains("egui::Window::new(\"Asistente\")"));
    assert!(assistant_source.contains("Configuración"));
    assert!(assistant_source.contains("Icon::Settings"));
    assert!(assistant_source.contains("TopBottomPanel::bottom(\"grafito_assistant_composer\")"));
    assert!(
        assistant_source.contains("TopBottomPanel::bottom(\"grafito_assistant_compact_panel\")")
    );
    assert!(assistant_source.contains("assistant_uses_bottom_sheet"));
    // El composer ya no envuelve todo en un ScrollArea que desborda: sólo el textarea tiene scroll acotado
    // o el editor es de altura fija. Verificamos que exista un panel composer y que no haya un scroll envolvente obsoleto.
    assert!(assistant_source.contains("grafito_assistant_composer"));
    assert!(!assistant_source.contains("Destino: {} / {}"));
    assert!(assistant_source.contains("stick_to_bottom(true)"));
    let conversation_start = assistant_source
        .find("grafito_assistant_conversation")
        .expect("assistant conversation scroll area");
    let conversation_end = assistant_source[conversation_start..]
        .find("fn retain_first_assistant_action")
        .map(|offset| conversation_start + offset)
        .expect("assistant conversation scroll boundary");
    let conversation_scroll = &assistant_source[conversation_start..conversation_end];
    assert!(conversation_scroll.contains(".auto_shrink([false, true])"));
    assert!(assistant_source.contains(".truncate()"));
    assert!(assistant_source.contains("fn draw_conversation_turn"));
    let turn_start = assistant_source
        .find("fn draw_conversation_turn")
        .expect("assistant conversation renderer");
    let pending_start = assistant_source[turn_start..]
        .find("fn draw_pending_indicator")
        .map(|offset| turn_start + offset)
        .expect("assistant pending renderer");
    let turn_source = &assistant_source[turn_start..pending_start];
    assert!(!turn_source.contains("assistant_bubble_width"));
    assert!(!turn_source.contains("let layout = if is_user"));
    assert!(turn_source.contains("conversation_turn_appearance"));
    assert!(turn_source.contains("egui::Frame::none()"));
    assert!(turn_source.contains("ui.set_min_width(ui.available_width())"));
    let response_start = assistant_source[pending_start..]
        .find("fn draw_assistant_response")
        .map(|offset| pending_start + offset)
        .expect("assistant response renderer");
    let pending_source = &assistant_source[pending_start..response_start];
    assert!(!pending_source.contains("assistant_bubble_width"));
    assert!(pending_source.contains("conversation_turn_appearance"));
    assert!(pending_source.contains("egui::Frame::none()"));
    assert!(assistant_source.contains("RetryProposalCorrection"));
    assert!(assistant_source.contains("Adjuntar imagen"));
    let proposal_start = assistant_source
        .find("fn draw_verified_assistant_proposal")
        .expect("verified proposal renderer");
    let proposal_end = assistant_source[proposal_start..]
        .find("fn verified_proposal")
        .map(|offset| proposal_start + offset)
        .expect("proposal lookup helper");
    let proposal_source = &assistant_source[proposal_start..proposal_end];
    assert!(proposal_source.contains("Button::new(\"Aplicar en Grafito\")"));
    assert!(!proposal_source.contains("Icon::Play"));
    assert_eq!(proposal_source.matches("ApplyProposal(").count(), 1);
    let composer_start = assistant_source
        .find("fn draw_assistant_composer")
        .expect("assistant composer function");
    let composer_end = assistant_source[composer_start..]
        .find("fn draw_conversation_turn")
        .map(|offset| composer_start + offset)
        .expect("assistant conversation renderer");
    let composer_source = &assistant_source[composer_start..composer_end];
    assert!(composer_source.contains("Button::new(\"Enviar\")"));
    assert!(!composer_source.contains("Icon::Play"));
    assert!(!assistant_source.contains("pub open: bool"));
    assert!(!app_source.contains("draw_canvas_assistant_affordance"));
    assert!(!icons_source.contains("Icon::Assistant"));
}

#[test]
fn assistant_uses_contextual_remote_interaction_without_transcription_or_local_copy() {
    let assistant_source = include_str!("../src/assistant.rs");

    assert!(assistant_source.contains("ProviderProfile::OpenCodeGo"));
    assert!(assistant_source.contains("ComboBox::from_id_salt(\"assistant_model\")"));
    assert!(assistant_source.contains("Función seleccionada"));
    assert!(assistant_source.contains("Adjuntar imagen"));
    assert!(!assistant_source.contains("Shift+Enter agrega una línea."));
    assert!(assistant_source.contains("AssistantUiAction::InsertCommand"));
    assert!(assistant_source.contains("AssistantUiAction::ApplyProposal"));
    assert!(!assistant_source.contains("assistant_response_proposals"));
    assert!(assistant_source.contains("proposal_code_block_indices"));
    assert!(assistant_source.contains("draw_pending_indicator"));
    assert!(assistant_source.contains("draw_assistant_empty_state"));
    assert!(assistant_source.contains("draw_math"));
    assert!(assistant_source.contains("parse_assistant_blocks"));
    assert!(!assistant_source.contains("text_edit_singleline(&mut state.model)"));
    assert!(!assistant_source.contains("Resolver localmente"));
    assert!(!assistant_source.contains("Transcripción de imagen"));
    assert!(!assistant_source.contains("local_only"));
}
