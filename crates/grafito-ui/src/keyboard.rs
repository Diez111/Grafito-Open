//! Math Keyboard — 5 tabs covering ALL functions.
//! Always docked at the bottom. Collapsible, compact mode available.

use egui::{Color32, Ui};

#[derive(Clone, Default)]
pub struct MathKeyboard {
    pub active_tab: usize,
    pub collapsed: bool,
    pub compact: bool,
}

const TAB_LABELS: &[&str] = &["123", "f(x)", "αβγ", "⌘ Cmd", "3D"];

type KeyDef = (&'static str, KeyAction);

enum KeyAction {
    Ins(&'static str),
    Paren(&'static str),
    Brack(&'static str),
    Exec(&'static str),
    Bs,
    Enter,
}

const NUMBERS: &[KeyDef] = &[
    ("7", KeyAction::Ins("7")),
    ("8", KeyAction::Ins("8")),
    ("9", KeyAction::Ins("9")),
    ("÷", KeyAction::Ins("/")),
    ("^", KeyAction::Ins("^")),
    ("sin", KeyAction::Paren("sin")),
    ("cos", KeyAction::Paren("cos")),
    ("tan", KeyAction::Paren("tan")),
    ("4", KeyAction::Ins("4")),
    ("5", KeyAction::Ins("5")),
    ("6", KeyAction::Ins("6")),
    ("×", KeyAction::Ins("*")),
    ("√", KeyAction::Paren("sqrt")),
    ("asin", KeyAction::Paren("asin")),
    ("acos", KeyAction::Paren("acos")),
    ("atan", KeyAction::Paren("atan")),
    ("1", KeyAction::Ins("1")),
    ("2", KeyAction::Ins("2")),
    ("3", KeyAction::Ins("3")),
    ("−", KeyAction::Ins("-")),
    ("π", KeyAction::Ins("pi")),
    ("ln", KeyAction::Paren("ln")),
    ("log", KeyAction::Paren("log10")),
    ("e", KeyAction::Ins("e")),
    ("0", KeyAction::Ins("0")),
    (".", KeyAction::Ins(".")),
    ("=", KeyAction::Ins("=")),
    ("+", KeyAction::Ins("+")),
    ("( )", KeyAction::Ins("()")),
    ("|x|", KeyAction::Paren("abs")),
    ("⌫", KeyAction::Bs),
    ("⏎", KeyAction::Enter),
];

const FUNCTIONS: &[KeyDef] = &[
    ("sin", KeyAction::Paren("sin")),
    ("cos", KeyAction::Paren("cos")),
    ("tan", KeyAction::Paren("tan")),
    ("sec", KeyAction::Paren("sec")),
    ("csc", KeyAction::Paren("csc")),
    ("cot", KeyAction::Paren("cot")),
    ("floor", KeyAction::Paren("floor")),
    ("ceil", KeyAction::Paren("ceil")),
    ("asin", KeyAction::Paren("asin")),
    ("acos", KeyAction::Paren("acos")),
    ("atan", KeyAction::Paren("atan")),
    ("sinh", KeyAction::Paren("sinh")),
    ("cosh", KeyAction::Paren("cosh")),
    ("tanh", KeyAction::Paren("tanh")),
    ("round", KeyAction::Paren("round")),
    ("sign", KeyAction::Paren("sign")),
    ("exp", KeyAction::Paren("exp")),
    ("ln", KeyAction::Paren("ln")),
    ("log", KeyAction::Paren("log10")),
    ("sqrt", KeyAction::Paren("sqrt")),
    ("cbrt", KeyAction::Paren("cbrt")),
    ("|x|", KeyAction::Paren("abs")),
    ("gamma", KeyAction::Paren("gamma")),
    ("erf", KeyAction::Paren("erf")),
    ("beta", KeyAction::Paren("beta")),
    ("bessel", KeyAction::Paren("BesselJ")),
    ("digamma", KeyAction::Paren("Digamma")),
    ("heavside", KeyAction::Paren("heaviside")),
    ("mod", KeyAction::Paren("mod")),
    ("min", KeyAction::Paren("min")),
    ("max", KeyAction::Paren("max")),
    ("clamp", KeyAction::Paren("clamp")),
];

const GREEK: &[KeyDef] = &[
    ("α", KeyAction::Ins("α")),
    ("β", KeyAction::Ins("β")),
    ("γ", KeyAction::Ins("γ")),
    ("δ", KeyAction::Ins("δ")),
    ("ε", KeyAction::Ins("ε")),
    ("ζ", KeyAction::Ins("ζ")),
    ("η", KeyAction::Ins("η")),
    ("θ", KeyAction::Ins("θ")),
    ("ι", KeyAction::Ins("ι")),
    ("κ", KeyAction::Ins("κ")),
    ("λ", KeyAction::Ins("λ")),
    ("μ", KeyAction::Ins("μ")),
    ("ν", KeyAction::Ins("ν")),
    ("ξ", KeyAction::Ins("ξ")),
    ("ο", KeyAction::Ins("ο")),
    ("π", KeyAction::Ins("π")),
    ("ρ", KeyAction::Ins("ρ")),
    ("σ", KeyAction::Ins("σ")),
    ("τ", KeyAction::Ins("τ")),
    ("υ", KeyAction::Ins("υ")),
    ("φ", KeyAction::Ins("φ")),
    ("χ", KeyAction::Ins("χ")),
    ("ψ", KeyAction::Ins("ψ")),
    ("ω", KeyAction::Ins("ω")),
    ("≤", KeyAction::Ins("<=")),
    ("≥", KeyAction::Ins(">=")),
    ("≠", KeyAction::Ins("!=")),
    ("≈", KeyAction::Ins("~")),
    ("±", KeyAction::Ins("±")),
    ("∞", KeyAction::Ins("inf")),
    ("∂", KeyAction::Ins("d")),
    ("∫", KeyAction::Ins("Integral[]")),
];

const COMMANDS: &[KeyDef] = &[
    ("Solve", KeyAction::Brack("Solve")),
    ("Derivada", KeyAction::Brack("Derivative")),
    ("Integral", KeyAction::Brack("Integral")),
    ("Limite", KeyAction::Brack("Limit")),
    ("Factor", KeyAction::Brack("Factor")),
    ("Expandir", KeyAction::Brack("Expand")),
    ("Simplif", KeyAction::Brack("Simplify")),
    ("Taylor", KeyAction::Brack("Taylor")),
    ("Punto", KeyAction::Brack("Point")),
    ("Recta", KeyAction::Brack("Line")),
    ("Circulo", KeyAction::Brack("Circle")),
    ("Poligono", KeyAction::Brack("Polygon")),
    ("Funcion", KeyAction::Brack("Function")),
    ("Segment", KeyAction::Brack("Segment")),
    ("Vector", KeyAction::Brack("Vector")),
    ("Rayo", KeyAction::Brack("Ray")),
    ("Histogram", KeyAction::Brack("Histogram")),
    ("Scatter", KeyAction::Brack("ScatterPlot")),
    ("BoxPlot", KeyAction::Brack("BoxPlot")),
    ("Regresion", KeyAction::Brack("LinearRegression")),
    ("Media", KeyAction::Brack("Mean")),
    ("Mediana", KeyAction::Brack("Median")),
    ("StdDev", KeyAction::Brack("StdDev")),
    ("Normal", KeyAction::Brack("Normal")),
    ("PtoMedio", KeyAction::Brack("Midpoint")),
    ("Intersec", KeyAction::Brack("Intersect")),
    ("Locus", KeyAction::Brack("Locus")),
    ("Tangente", KeyAction::Brack("Tangent")),
    ("Traslad", KeyAction::Brack("Translate")),
    ("Rotar", KeyAction::Brack("Rotate")),
    ("Reflejar", KeyAction::Brack("Reflect")),
    ("Dilatar", KeyAction::Brack("Dilate")),
];

const PRESETS_3D: &[KeyDef] = &[
    ("Sphere", KeyAction::Brack("Sphere")),
    ("Cube", KeyAction::Brack("Cube")),
    ("Cylinder", KeyAction::Brack("Cylinder")),
    ("Cone", KeyAction::Brack("Cone")),
    ("Pyramid", KeyAction::Brack("Pyramid")),
    ("Torus", KeyAction::Brack("Torus")),
    ("Moebius", KeyAction::Brack("Moebius")),
    ("Surface", KeyAction::Brack("Surface3D")),
    ("Point3D", KeyAction::Brack("Point3D")),
    ("Segment3D", KeyAction::Brack("Segment3D")),
    ("Curve3D", KeyAction::Brack("Curve3D")),
    ("Attractor", KeyAction::Exec("Lorenz[]")),
    ("Vector3D", KeyAction::Brack("VectorField3D")),
    ("CplxGrid", KeyAction::Brack("ComplexGrid")),
    ("Hypercube", KeyAction::Brack("Hypercube")),
    ("Hypersph", KeyAction::Brack("Hypersphere")),
    ("Lorenz[]", KeyAction::Exec("Lorenz[]")),
    ("Rossler[]", KeyAction::Exec("Rossler[]")),
    ("Thomas[]", KeyAction::Exec("Thomas[]")),
    ("Aizawa[]", KeyAction::Exec("Aizawa[]")),
    ("Chen[]", KeyAction::Exec("Chen[]")),
    ("Chua[]", KeyAction::Exec("Chua[]")),
    ("Dadras[]", KeyAction::Exec("Dadras[]")),
    ("Halvors[]", KeyAction::Exec("Halvorsen[]")),
    ("Mandelbr", KeyAction::Exec("Mandelbrot[]")),
    ("Julia[]", KeyAction::Exec("Julia[]")),
    ("BurnShip", KeyAction::Exec("BurningShip[]")),
    ("DomColor", KeyAction::Brack("DomainColoring")),
    ("HeatMap", KeyAction::Brack("HeatMap")),
    ("Contour", KeyAction::Brack("Contour")),
    ("PhasePort", KeyAction::Brack("PhasePortrait")),
    ("ODE[]", KeyAction::Brack("ODE")),
];

impl MathKeyboard {
    pub fn show(
        &mut self,
        ui: &mut Ui,
        input_text: &mut String,
        accent: Color32,
        on_execute: &mut bool,
    ) {
        let is_dark = ui.visuals().dark_mode;
        let bg = if is_dark {
            Color32::from_rgb(28, 28, 36)
        } else {
            Color32::from_rgb(244, 245, 250)
        };
        let _sep = if is_dark {
            Color32::from_rgb(50, 50, 58)
        } else {
            Color32::from_rgb(210, 210, 215)
        };
        let btn_bg = if is_dark {
            Color32::from_rgb(42, 42, 52)
        } else {
            Color32::from_rgb(250, 250, 252)
        };
        let _btn_hover = if is_dark {
            Color32::from_rgb(60, 60, 70)
        } else {
            Color32::from_rgb(235, 240, 250)
        };
        let txt = if is_dark {
            Color32::WHITE
        } else {
            Color32::from_rgb(30, 30, 30)
        };
        let txt_dim = if is_dark {
            Color32::from_gray(160)
        } else {
            Color32::from_gray(100)
        };

        egui::Frame::none()
            .fill(bg)
            .inner_margin(egui::Margin::symmetric(6.0, 4.0))
            .show(ui, |ui| {
                // Tab bar row
                ui.horizontal(|ui| {
                    for (i, label) in TAB_LABELS.iter().enumerate() {
                        let active = self.active_tab == i;
                        let c = if active { accent } else { txt_dim };
                        let resp = ui.selectable_label(
                            active,
                            egui::RichText::new(*label).size(12.0).color(c),
                        );
                        if resp.clicked() {
                            self.active_tab = i;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let s = egui::vec2(24.0, 18.0);
                        if ui
                            .add_sized(
                                s,
                                egui::Button::new(
                                    egui::RichText::new("−").size(14.0).color(txt_dim),
                                )
                                .frame(false),
                            )
                            .clicked()
                        {
                            self.collapsed = !self.collapsed;
                        }
                        if ui
                            .add_sized(
                                s,
                                egui::Button::new(
                                    egui::RichText::new("□").size(14.0).color(txt_dim),
                                )
                                .frame(false),
                            )
                            .clicked()
                        {
                            input_text.clear();
                        }
                        if ui
                            .add_sized(
                                s,
                                egui::Button::new(
                                    egui::RichText::new("⌨").size(14.0).color(txt_dim),
                                )
                                .frame(false),
                            )
                            .clicked()
                        {
                            self.compact = !self.compact;
                        }
                    });
                });

                if self.collapsed {
                    return;
                }

                ui.add_space(2.0);
                let buttons = match self.active_tab {
                    0 => NUMBERS,
                    1 => FUNCTIONS,
                    2 => GREEK,
                    3 => COMMANDS,
                    4 => PRESETS_3D,
                    _ => NUMBERS,
                };
                let cols = 8;
                let btn_w = (ui.available_width() / cols as f32) - 4.0;
                let btn_h = 28.0;

                egui::Grid::new("kbd_grid")
                    .spacing(egui::vec2(2.0, 2.0))
                    .show(ui, |ui| {
                        for (i, (label, action)) in buttons.iter().enumerate() {
                            if i > 0 && i % cols == 0 {
                                ui.end_row();
                            }
                            let btn = egui::Button::new(
                                egui::RichText::new(*label).size(11.0).color(txt),
                            )
                            .fill(btn_bg)
                            .min_size(egui::vec2(btn_w, btn_h));
                            let resp = ui.add(btn);
                            if resp.clicked() {
                                match action {
                                    KeyAction::Ins(s) => input_text.push_str(s),
                                    KeyAction::Paren(s) => {
                                        input_text.push_str(&format!("{}()", s));
                                    }
                                    KeyAction::Brack(s) => {
                                        input_text.push_str(&format!("{}[]", s));
                                    }
                                    KeyAction::Exec(s) => {
                                        *input_text = s.to_string();
                                        *on_execute = true;
                                    }
                                    KeyAction::Bs => {
                                        input_text.pop();
                                    }
                                    KeyAction::Enter => {
                                        *on_execute = true;
                                    }
                                }
                            }
                            if resp.hovered() {
                                ui.ctx()
                                    .output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                            }
                        }
                    });
            });
    }
}
