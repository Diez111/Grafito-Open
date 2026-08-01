# Polished Math Assistant Design

## Problem

The assistant treats inline LaTeX as source code and reduces display equations
to monospaced replacement text. Its empty conversation has no native onboarding
and the composer reserves unstable, excessive vertical space, making a narrow
desktop panel feel crowded and less trustworthy.

## Reframe

The goal is not to embed a full TeX engine. Grafito needs a legible,
dependency-free mathematical reading surface that preserves the assistant's
fast native UI and lets users distinguish prose, code, and mathematics at a
glance.

## Approach

Implement a bounded private LaTeX subset in `grafito-ui::assistant`. A small
recursive parser recognizes groups, nested fractions, square roots, scripts,
common Greek symbols, operators, relations, and ignored sizing delimiters.
Display mathematics uses a custom egui painter layout with fraction bars,
radical overbars, and scaled scripts. Inline mathematics uses the same parser
for a clean proportional linear fallback, rather than code styling. Invalid or
unsupported input remains visible as literal source without panicking.

The transcript renders a compact empty state rather than treating onboarding as
a conversation turn. The composer gets a declared minimum height, moves quick
prompts to that empty state, reduces idle metadata, and shows detailed budgets
only on hover or near their limit.

## Scope

In scope:

1. Common LaTeX fractions, roots, powers, subscripts, Greek symbols,
   relations, and operators in assistant responses.
2. Horizontal scrolling for long display formulae.
3. Compact empty-state onboarding and a predictable composer allocation.
4. Regression tests for parsing, fallback, and layout metrics.

Out of scope:

1. Full TeX, matrices, alignment environments, arbitrary macros, or external
   font/math dependencies.
2. Changes to the mathematical command language or provider protocol.

## Technical Design

`MathExpr` holds `Text`, `Row`, `Fraction`, `Root`, and `Script` nodes. Parsing
uses a depth and node budget, maps known commands to Unicode, and preserves
unknown commands literally. `MathLayout` stores measured galleys, relative
positions, baselines, and vector rules. `draw_math` allocates exact egui space,
paints leaves with `Painter::galley`, and paints fraction/root rules. Display
cards host it in a horizontal `ScrollArea`; inline text continues to use a
wrapping `LayoutJob` with math formatted as proportional accent text.

`draw_assistant_empty_state` renders onboarding only with an empty transcript.
`draw_assistant_composer` uses one idle editor row and suppresses attachment and
byte labels unless meaningful. The bottom panel has a stable minimum height.

## Acceptance Criteria

1. `\frac{a_i^2}{\sqrt{b}}`, `\alpha \leq \beta`, and common symbols display
   without raw commands or code-style backgrounds.
2. Nested fractions, roots, scripts, malformed groups, and unknown commands do
   not panic or silently discard source.
3. Long equations never clip in the narrow assistant panel.
4. Empty assistant onboarding is visible without consuming conversation history.
5. The composer retains Enter/Shift+Enter behavior while consuming less idle
   space and no longer shifts height on its first frame.

## Test Strategy

Unit-test parser-to-linear fallback outputs, nested structures, invalid input,
and mathematical layout dimensions. Test empty-state eligibility and composer
height helpers. Run focused UI tests, then workspace formatting, clippy, tests,
release build, package verification, and installation.

## Risks

1. Native font fallback may lack an uncommon glyph. The supported subset uses
   broadly available Unicode and retains source on unknown commands.
2. A handwritten layout can overflow. Exact allocation plus horizontal display
   scrolling prevents clipping; parser depth and node limits bound work.
3. Full TeX expectations exceed a native desktop panel. Unsupported structures
   intentionally fall back visibly rather than rendering incorrect mathematics.
