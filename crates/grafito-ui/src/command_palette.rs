//! Grafito Command Palette — Ctrl+K quick command search.

pub struct PaletteCommand {
    pub name: &'static str,
    pub category: &'static str,
    pub syntax_hint: &'static str,
}

pub fn all_commands() -> Vec<PaletteCommand> {
    vec![
        // Basic tools
        PaletteCommand {
            name: "Point Tool",
            category: "Tools",
            syntax_hint: "Click on canvas | (x, y)",
        },
        PaletteCommand {
            name: "Line Tool",
            category: "Tools",
            syntax_hint: "Click two points | A = (x1,y1), B = (x2,y2)",
        },
        PaletteCommand {
            name: "Circle Tool",
            category: "Tools",
            syntax_hint: "Click center + edge | Circle[(x,y), r]",
        },
        PaletteCommand {
            name: "Polygon Tool",
            category: "Tools",
            syntax_hint: "Click vertices",
        },
        PaletteCommand {
            name: "Function Tool",
            category: "Tools",
            syntax_hint: "f(x) = expr",
        },
        // 2D Geometry
        PaletteCommand {
            name: "Ellipse",
            category: "Create",
            syntax_hint: "Ellipse[(cx,cy), rx, ry]",
        },
        PaletteCommand {
            name: "Parabola",
            category: "Create",
            syntax_hint: "Parabola[(vx,vy), p]",
        },
        PaletteCommand {
            name: "Hyperbola",
            category: "Create",
            syntax_hint: "Hyperbola[(cx,cy), a, b]",
        },
        PaletteCommand {
            name: "RegularPolygon",
            category: "Create",
            syntax_hint: "RegularPolygon[(cx,cy), n, r]",
        },
        // Transformations
        PaletteCommand {
            name: "Translate",
            category: "Transform",
            syntax_hint: "Translate[obj, (dx,dy)]",
        },
        PaletteCommand {
            name: "Rotate",
            category: "Transform",
            syntax_hint: "Rotate[obj, angle_deg]",
        },
        PaletteCommand {
            name: "Dilate",
            category: "Transform",
            syntax_hint: "Dilate[obj, factor, (cx,cy)]",
        },
        PaletteCommand {
            name: "Reflect",
            category: "Transform",
            syntax_hint: "Reflect[obj, (ax,ay), (bx,by)]",
        },
        // Construction
        PaletteCommand {
            name: "Tangent",
            category: "Construct",
            syntax_hint: "Tangent[(cx,cy), r, (px,py)]",
        },
        PaletteCommand {
            name: "PerpendicularBisector",
            category: "Construct",
            syntax_hint: "PerpendicularBisector[(x1,y1), (x2,y2)]",
        },
        PaletteCommand {
            name: "AngleBisector",
            category: "Construct",
            syntax_hint: "AngleBisector[p1, vertex, p2]",
        },
        PaletteCommand {
            name: "Midpoint",
            category: "Construct",
            syntax_hint: "Midpoint[(x1,y1), (x2,y2)]",
        },
        PaletteCommand {
            name: "Line",
            category: "Construct",
            syntax_hint: "Line[(x1,y1), (x2,y2)]",
        },
        PaletteCommand {
            name: "Segment",
            category: "Construct",
            syntax_hint: "Segment[(x1,y1), (x2,y2)]",
        },
        PaletteCommand {
            name: "Vector",
            category: "Construct",
            syntax_hint: "Vector[(x1,y1), (x2,y2)]",
        },
        PaletteCommand {
            name: "Ray",
            category: "Construct",
            syntax_hint: "Ray[(x1,y1), (x2,y2)]",
        },
        // CAS
        PaletteCommand {
            name: "Derivative",
            category: "CAS",
            syntax_hint: "Derivative[expr]",
        },
        PaletteCommand {
            name: "Integral",
            category: "CAS",
            syntax_hint: "Integral[expr, a, b]",
        },
        PaletteCommand {
            name: "Solve",
            category: "CAS",
            syntax_hint: "Solve[expr, a, b]",
        },
        PaletteCommand {
            name: "Limit",
            category: "CAS",
            syntax_hint: "Limit[expr, x0]",
        },
        PaletteCommand {
            name: "Factor",
            category: "CAS",
            syntax_hint: "Factor[expr]",
        },
        PaletteCommand {
            name: "Expand",
            category: "CAS",
            syntax_hint: "Expand[expr]",
        },
        PaletteCommand {
            name: "Simplify",
            category: "CAS",
            syntax_hint: "Simplify[expr]",
        },
        PaletteCommand {
            name: "Taylor",
            category: "CAS",
            syntax_hint: "Taylor[expr, x, x0, order]",
        },
        // Matrices
        PaletteCommand {
            name: "Determinant",
            category: "Matrices",
            syntax_hint: "Determinant[[a,b],[c,d]]",
        },
        PaletteCommand {
            name: "Inverse",
            category: "Matrices",
            syntax_hint: "Inverse[[a,b],[c,d]]",
        },
        PaletteCommand {
            name: "SolveSystem",
            category: "Matrices",
            syntax_hint: "SolveSystem[[a,b],[c,d],[e,f]]",
        },
        // Probability
        PaletteCommand {
            name: "Normal",
            category: "Probability",
            syntax_hint: "Normal[mu, sigma]",
        },
        PaletteCommand {
            name: "Binomial",
            category: "Probability",
            syntax_hint: "Binomial[n, p, k]",
        },
        PaletteCommand {
            name: "Poisson",
            category: "Probability",
            syntax_hint: "Poisson[lambda, k]",
        },
        // Statistics
        PaletteCommand {
            name: "Histogram",
            category: "Statistics",
            syntax_hint: "Histogram[{data}, bins]",
        },
        PaletteCommand {
            name: "ScatterPlot",
            category: "Statistics",
            syntax_hint: "ScatterPlot[{xs}, {ys}]",
        },
        PaletteCommand {
            name: "BoxPlot",
            category: "Statistics",
            syntax_hint: "BoxPlot[{data}]",
        },
        PaletteCommand {
            name: "LinearRegression",
            category: "Statistics",
            syntax_hint: "LinearRegression[{xs}, {ys}]",
        },
        PaletteCommand {
            name: "Mean",
            category: "Statistics",
            syntax_hint: "Mean[{data}]",
        },
        PaletteCommand {
            name: "Median",
            category: "Statistics",
            syntax_hint: "Median[{data}]",
        },
        PaletteCommand {
            name: "StdDev",
            category: "Statistics",
            syntax_hint: "StdDev[{data}]",
        },
        PaletteCommand {
            name: "Correlation",
            category: "Statistics",
            syntax_hint: "Correlation[{xs}, {ys}]",
        },
        // Attractors
        PaletteCommand {
            name: "Lorenz",
            category: "Attractors",
            syntax_hint: "Lorenz[] | Lorenz[sigma, rho, beta]",
        },
        PaletteCommand {
            name: "Rossler",
            category: "Attractors",
            syntax_hint: "Rossler[] | Rossler[a, b, c]",
        },
        PaletteCommand {
            name: "Thomas (Butterfly)",
            category: "Attractors",
            syntax_hint: "Thomas[] | Butterfly[]",
        },
        PaletteCommand {
            name: "Aizawa",
            category: "Attractors",
            syntax_hint: "Aizawa[]",
        },
        PaletteCommand {
            name: "Chen",
            category: "Attractors",
            syntax_hint: "Chen[]",
        },
        PaletteCommand {
            name: "Halvorsen",
            category: "Attractors",
            syntax_hint: "Halvorsen[]",
        },
        PaletteCommand {
            name: "Dadras",
            category: "Attractors",
            syntax_hint: "Dadras[]",
        },
        PaletteCommand {
            name: "Chua",
            category: "Attractors",
            syntax_hint: "Chua[]",
        },
        // Fractals
        PaletteCommand {
            name: "Mandelbrot",
            category: "Fractals",
            syntax_hint: "Mandelbrot[] | Mandelbrot[max_iter]",
        },
        PaletteCommand {
            name: "Julia",
            category: "Fractals",
            syntax_hint: "Julia[cr, ci] | Julia[cr, ci, max_iter]",
        },
        PaletteCommand {
            name: "BurningShip",
            category: "Fractals",
            syntax_hint: "BurningShip[]",
        },
        // 4D
        PaletteCommand {
            name: "Hypercube",
            category: "4D",
            syntax_hint: "Hypercube[] | Hypercube[a1, a2, a3]",
        },
        PaletteCommand {
            name: "Hypersphere",
            category: "4D",
            syntax_hint: "Hypersphere[]",
        },
        // 3D
        PaletteCommand {
            name: "Curve3D",
            category: "3D",
            syntax_hint: "Curve3D[(x(t),y(t),z(t)), t, tmin, tmax]",
        },
        PaletteCommand {
            name: "Surface3D",
            category: "3D",
            syntax_hint: "Surface3D[z=f(x,y), xmin, xmax, ymin, ymax]",
        },
        PaletteCommand {
            name: "Extrude",
            category: "3D",
            syntax_hint: "Extrude[polygon_label, height]",
        },
        PaletteCommand {
            name: "VectorField3D",
            category: "3D",
            syntax_hint: "VectorField3D[u, v, w]",
        },
        // File
        PaletteCommand {
            name: "Save",
            category: "File",
            syntax_hint: "Save current document",
        },
        PaletteCommand {
            name: "Export SVG",
            category: "File",
            syntax_hint: "Export vector graphics",
        },
        PaletteCommand {
            name: "Export PNG",
            category: "File",
            syntax_hint: "Export raster image",
        },
        PaletteCommand {
            name: "Export TikZ",
            category: "File",
            syntax_hint: "Export LaTeX TikZ code",
        },
        // View
        PaletteCommand {
            name: "Zoom to Fit",
            category: "View",
            syntax_hint: "Fit all objects in view",
        },
        PaletteCommand {
            name: "Toggle Grid",
            category: "View",
            syntax_hint: "Show/hide coordinate grid",
        },
        PaletteCommand {
            name: "Toggle Dark Mode",
            category: "View",
            syntax_hint: "Switch light/dark theme",
        },
    ]
}

#[derive(Default)]
pub struct CommandPaletteState {
    pub open: bool,
    pub search: String,
    pub selected_index: usize,
}

impl CommandPaletteState {
    pub fn filtered_commands(&self) -> Vec<PaletteCommand> {
        let all = all_commands();
        let search_lower = self.search.to_lowercase();

        all.into_iter()
            .filter(|cmd| {
                search_lower.is_empty()
                    || cmd.name.to_lowercase().contains(&search_lower)
                    || cmd.category.to_lowercase().contains(&search_lower)
            })
            .collect()
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<String> {
        if !self.open {
            return None;
        }

        let mut selected_command = None;

        egui::Window::new("Command Palette")
            .collapsible(false)
            .resizable(false)
            .default_pos([ctx.screen_rect().width() * 0.3, 100.0])
            .default_width(ctx.screen_rect().width() * 0.4)
            .show(ctx, |ui| {
                // Search field
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    let response = ui.text_edit_singleline(&mut self.search);
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let filtered = self.filtered_commands();
                        if let Some(cmd) = filtered.get(self.selected_index) {
                            selected_command = Some(cmd.name.to_string());
                        }
                    }
                    response.request_focus();
                });

                ui.separator();

                // Command list
                let filtered = self.filtered_commands();
                if filtered.is_empty() {
                    ui.label("No commands found");
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(400.0)
                        .show(ui, |ui| {
                            for (i, cmd) in filtered.iter().enumerate() {
                                let is_selected = i == self.selected_index;
                                let response = ui.selectable_label(
                                    is_selected,
                                    format!("{} — {}", cmd.name, cmd.category),
                                );

                                if response.clicked() {
                                    selected_command = Some(cmd.name.to_string());
                                }

                                if response.hovered() {
                                    ui.label(egui::RichText::new(cmd.syntax_hint).small().weak());
                                }
                            }
                        });
                }

                // Keyboard navigation
                if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                    let filtered = self.filtered_commands();
                    if self.selected_index < filtered.len().saturating_sub(1) {
                        self.selected_index += 1;
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) && self.selected_index > 0 {
                    self.selected_index -= 1;
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.open = false;
                }
            });

        if selected_command.is_some() {
            self.open = false;
            self.search.clear();
            self.selected_index = 0;
        }

        selected_command
    }
}
