---
name: rust-ui-ux
description: UI escandinava egui para Grafito. Usa tokens TYPE/SPACE/RADIUS y progressive disclosure 5/8/17.
---

# rust-ui-ux

## Tokens (fuente única `grafito-ui/src/tokens.rs` + `theme.rs` para color)
- `TYPE_2XS/XS 11, SM 12, BASE 15, MD 16, LG 19, XL 24, XXL 28 (ratio ~1.25)`, `SPACE 4/8/12/16/24/40 base4`, `RADIUS 8/12/16`, `ICON 16/20/24/32`. Piso 11.0 (9.0→11.0 migrado).
- Paleta real: canvas `#FAFAF9`, panel `#FFF`, separator `#E8E8E6`, acento sage `#6B7A6F` (ver `theme.rs` DARK/LIGHT). Excepción documentada: compact top-chrome `button_padding 10×4` (ui.rs).
- `Inter (+Variable embebida) + SF Mono`, contraste ≥4.5 testeado en `theme.rs`, sin `System` aún (P1), sin `NO_COLOR/--theme` aún (P1).
- Colores de DATOS (series/análisis/ejes) quedan literales a propósito: piden paleta semántica P2, no `Theme`.

## Reglas Piel
- `fn render(&Estado) -> Frame`; cero I/O/spawn en `Ui::`.
- Toolbar 17 grupos: PRIMARY 5 / SECONDARY 8 / UNIVERSITY 17.
- Assistant panel 340..460, composer 88..260, rail 60px ≥1360, drawer 292..440.
- Estados obligatorios: `default/hover/focus-visible/disabled/loading/empty/error` + microcopy hygge, no `panic!` rojo gigante.
- `NO_COLOR` + `--theme`, `prefers-reduced-motion`, transform/opacity solo.

## Skills externas bajo demanda (ver docs/SKILLS-CATALOG.md §1)
`ui-ux-pro-max`, `frontend-design`, `design-tokens(Scandinavian)`, `ui-visual-composition`, `minimalist-ui/high-end-visual-design`, `web-design-guidelines` (gate 100 reglas), `wcag-audit`, `ux-writing`, `macos+web` platform, `figma-implement-design` si hay Figma.
