#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
const PRODUCTION_SOURCES: &[(&str, &str)] = &[
    ("commands.rs", include_str!("../src/commands.rs")),
    (
        "assistant_plan.rs",
        include_str!("../src/assistant_plan.rs"),
    ),
    (
        "assistant_context.rs",
        include_str!("../src/assistant_context.rs"),
    ),
    ("lib.rs", include_str!("../src/lib.rs")),
];

const LOSSY_INSERTION_CALLS: &[&str] = &[
    ".add_object(",
    ".add_point(",
    ".add_constructed_object(",
    ".add_constructed_object_with_params(",
    ".add_ellipse_by_foci_constraint(",
    ".add_parabola_by_focus_directrix_constraint(",
    ".add_hyperbola_by_foci_constraint(",
    ".add_conic_by_five_points_constraint(",
];

#[test]
fn production_command_code_uses_only_fallible_insertion_apis() {
    for (path, source) in PRODUCTION_SOURCES {
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        for (line_number, line) in production.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for forbidden in LOSSY_INSERTION_CALLS {
                assert!(
                    !line.contains(forbidden),
                    "{path}:{} uses lossy insertion call {forbidden}: {}",
                    line_number + 1,
                    line.trim()
                );
            }
        }
    }
}
