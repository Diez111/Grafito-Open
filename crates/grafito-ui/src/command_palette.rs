//! Paleta de Comandos de Grafito — búsqueda rápida de comandos con Ctrl+K.

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
            category: "Herramientas",
            syntax_hint: "Clic en el lienzo | (x, y)",
        },
        PaletteCommand {
            name: "Line Tool",
            category: "Herramientas",
            syntax_hint: "Clic dos puntos | A = (x1,y1), B = (x2,y2)",
        },
        PaletteCommand {
            name: "Circle Tool",
            category: "Herramientas",
            syntax_hint: "Clic centro + borde | Circle[(x,y), r]",
        },
        PaletteCommand {
            name: "Polygon Tool",
            category: "Herramientas",
            syntax_hint: "Clic vértices",
        },
        PaletteCommand {
            name: "Function Tool",
            category: "Herramientas",
            syntax_hint: "f(x) = expr",
        },
        PaletteCommand {
            name: "Pencil",
            category: "Herramientas",
            syntax_hint: "Clic sostenido y arrastrar para dibujar a mano alzada",
        },
        PaletteCommand {
            name: "Eraser",
            category: "Herramientas",
            syntax_hint: "Clic o arrastrar para borrar objetos",
        },
        // 2D Geometry
        PaletteCommand {
            name: "Ellipse",
            category: "Crear",
            syntax_hint: "Ellipse[(cx,cy), rx, ry]",
        },
        PaletteCommand {
            name: "Parabola",
            category: "Crear",
            syntax_hint: "Parabola[(vx,vy), p]",
        },
        PaletteCommand {
            name: "Hyperbola",
            category: "Crear",
            syntax_hint: "Hyperbola[(cx,cy), a, b]",
        },
        PaletteCommand {
            name: "RegularPolygon",
            category: "Crear",
            syntax_hint: "RegularPolygon[(cx,cy), n, r]",
        },
        // Transformations
        PaletteCommand {
            name: "Translate",
            category: "Transformar",
            syntax_hint: "Translate[obj, (dx,dy)]",
        },
        PaletteCommand {
            name: "Rotate",
            category: "Transformar",
            syntax_hint: "Rotate[obj, angle_deg]",
        },
        PaletteCommand {
            name: "Dilate",
            category: "Transformar",
            syntax_hint: "Dilate[obj, factor, (cx,cy)]",
        },
        PaletteCommand {
            name: "Reflect",
            category: "Transformar",
            syntax_hint: "Reflect[obj, (ax,ay), (bx,by)]",
        },
        // Construction
        PaletteCommand {
            name: "Tangent",
            category: "Construir",
            syntax_hint: "Tangent[(cx,cy), r, (px,py)]",
        },
        PaletteCommand {
            name: "PerpendicularBisector",
            category: "Construir",
            syntax_hint: "PerpendicularBisector[(x1,y1), (x2,y2)]",
        },
        PaletteCommand {
            name: "AngleBisector",
            category: "Construir",
            syntax_hint: "AngleBisector[p1, vertex, p2]",
        },
        PaletteCommand {
            name: "Midpoint",
            category: "Construir",
            syntax_hint: "Midpoint[(x1,y1), (x2,y2)]",
        },
        PaletteCommand {
            name: "Line",
            category: "Construir",
            syntax_hint: "Line[(x1,y1), (x2,y2)]",
        },
        PaletteCommand {
            name: "Segment",
            category: "Construir",
            syntax_hint: "Segment[(x1,y1), (x2,y2)]",
        },
        PaletteCommand {
            name: "Vector",
            category: "Construir",
            syntax_hint: "Vector[(x1,y1), (x2,y2)]",
        },
        PaletteCommand {
            name: "Ray",
            category: "Construir",
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
        // Análisis
        PaletteCommand {
            name: "Root",
            category: "Análisis",
            syntax_hint: "Root[f]",
        },
        PaletteCommand {
            name: "Extremum",
            category: "Análisis",
            syntax_hint: "Extremum[f]",
        },
        PaletteCommand {
            name: "Inflection",
            category: "Análisis",
            syntax_hint: "Inflection[f]",
        },
        PaletteCommand {
            name: "YIntercept",
            category: "Análisis",
            syntax_hint: "YIntercept[f]",
        },
        PaletteCommand {
            name: "XIntercept",
            category: "Análisis",
            syntax_hint: "XIntercept[f]  (raíces con cualquier curva)",
        },
        PaletteCommand {
            name: "Intersect",
            category: "Análisis",
            syntax_hint: "Intersect[a, b]  (cualquier par de curvas)",
        },
        PaletteCommand {
            name: "Analyze",
            category: "Análisis",
            syntax_hint: "Analyze[f]",
        },
        PaletteCommand {
            name: "Image",
            category: "Crear",
            syntax_hint: "Image[ruta/al/archivo.png]  o  clic en el lienzo",
        },
        // Curvas avanzadas
        PaletteCommand {
            name: "ParametricCurve2D",
            category: "Crear",
            syntax_hint: "ParametricCurve2D[x(t), y(t), t0, t1]",
        },
        PaletteCommand {
            name: "PolarCurve",
            category: "Crear",
            syntax_hint: "PolarCurve[r(t), t0, t1]",
        },
        PaletteCommand {
            name: "ImplicitCurve",
            category: "Crear",
            syntax_hint: "ImplicitCurve[f(x,y) = c]",
        },
        PaletteCommand {
            name: "ComplexMapping",
            category: "Crear",
            syntax_hint: "ComplexMapping[expr_compleja, target]",
        },
        PaletteCommand {
            name: "VectorField2D",
            category: "Crear",
            syntax_hint: "VectorField2D[u(x,y), v(x,y)]",
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
            category: "Probabilidad",
            syntax_hint: "Normal[mu, sigma]",
        },
        PaletteCommand {
            name: "Binomial",
            category: "Probabilidad",
            syntax_hint: "Binomial[n, p, k]",
        },
        PaletteCommand {
            name: "Poisson",
            category: "Probabilidad",
            syntax_hint: "Poisson[lambda, k]",
        },
        // Statistics
        PaletteCommand {
            name: "Histogram",
            category: "Estadística",
            syntax_hint: "Histogram[{data}, bins]",
        },
        PaletteCommand {
            name: "ScatterPlot",
            category: "Estadística",
            syntax_hint: "ScatterPlot[{xs}, {ys}]",
        },
        PaletteCommand {
            name: "BoxPlot",
            category: "Estadística",
            syntax_hint: "BoxPlot[{data}]",
        },
        PaletteCommand {
            name: "LinearRegression",
            category: "Estadística",
            syntax_hint: "LinearRegression[{xs}, {ys}]",
        },
        PaletteCommand {
            name: "Mean",
            category: "Estadística",
            syntax_hint: "Mean[{data}]",
        },
        PaletteCommand {
            name: "Median",
            category: "Estadística",
            syntax_hint: "Median[{data}]",
        },
        PaletteCommand {
            name: "StdDev",
            category: "Estadística",
            syntax_hint: "StdDev[{data}]",
        },
        PaletteCommand {
            name: "Correlation",
            category: "Estadística",
            syntax_hint: "Correlation[{xs}, {ys}]",
        },
        // Attractors
        PaletteCommand {
            name: "Lorenz",
            category: "Atractores",
            syntax_hint: "Lorenz[] | Lorenz[sigma, rho, beta]",
        },
        PaletteCommand {
            name: "Rossler",
            category: "Atractores",
            syntax_hint: "Rossler[] | Rossler[a, b, c]",
        },
        PaletteCommand {
            name: "Thomas (Butterfly)",
            category: "Atractores",
            syntax_hint: "Thomas[] | Butterfly[]",
        },
        PaletteCommand {
            name: "Aizawa",
            category: "Atractores",
            syntax_hint: "Aizawa[]",
        },
        PaletteCommand {
            name: "Chen",
            category: "Atractores",
            syntax_hint: "Chen[]",
        },
        PaletteCommand {
            name: "Halvorsen",
            category: "Atractores",
            syntax_hint: "Halvorsen[]",
        },
        PaletteCommand {
            name: "Dadras",
            category: "Atractores",
            syntax_hint: "Dadras[]",
        },
        PaletteCommand {
            name: "Chua",
            category: "Atractores",
            syntax_hint: "Chua[]",
        },
        // Fractals
        PaletteCommand {
            name: "Mandelbrot",
            category: "Fractales",
            syntax_hint: "Mandelbrot[] | Mandelbrot[max_iter]",
        },
        PaletteCommand {
            name: "Julia",
            category: "Fractales",
            syntax_hint: "Julia[cr, ci] | Julia[cr, ci, max_iter]",
        },
        PaletteCommand {
            name: "BurningShip",
            category: "Fractales",
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
            category: "Archivo",
            syntax_hint: "Guardar documento actual",
        },
        PaletteCommand {
            name: "Export SVG",
            category: "Archivo",
            syntax_hint: "Exportar gráficos vectoriales",
        },
        PaletteCommand {
            name: "Export PNG",
            category: "Archivo",
            syntax_hint: "Exportar imagen raster",
        },
        PaletteCommand {
            name: "Export TikZ",
            category: "Archivo",
            syntax_hint: "Exportar código LaTeX TikZ",
        },
        // View
        PaletteCommand {
            name: "Zoom to Fit",
            category: "Vista",
            syntax_hint: "Ajustar todos los objetos a la vista",
        },
        PaletteCommand {
            name: "Toggle Grid",
            category: "Vista",
            syntax_hint: "Mostrar/ocultar cuadrícula",
        },
        PaletteCommand {
            name: "Toggle Dark Mode",
            category: "Vista",
            syntax_hint: "Cambiar tema claro/oscuro",
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

        egui::Window::new("Paleta de Comandos")
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
                    ui.label("No se encontraron comandos");
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
