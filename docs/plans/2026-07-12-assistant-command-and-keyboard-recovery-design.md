# Assistant Command and Keyboard Recovery Design

## Problem

Assistant responses can present malformed command cards as actionable. In particular,
`Function[Sin[x] / (x^2 + 1)]` is accepted as a command but its expression does not
compile, so the renderer repeatedly samples invalid values and produces no curve. The
assistant also fails to make edited commands discoverable, and the mathematical keyboard
is automatically hidden at short viewport heights or in selected perspectives.

## Reframe

The goal is not to accept arbitrary model text. It is to guarantee one of two clear,
bounded outcomes: a reviewed command creates visible geometry, or the UI rejects it with
an actionable error before expensive rendering begins. The assistant must remain a
reviewed-command interface, not an autonomous executor.

## Approach

Use a focused, layered repair:

1. Normalize Mathematica-style brackets only inside mathematical expressions, so common
   provider output such as `Sin[x]` becomes a normal function call before every CPU/GPU
   evaluator sees it.
2. Make function sampling stop immediately when neither the native AST nor the compiled
   fallback can parse an expression. Valid but non-finite expressions, such as `1/0`,
   remain represented as gaps.
3. Require a complete, balanced `grafito` fence before rendering command actions; enforce
   exact arity for the assistant-exposed `Function` and `ParametricCurve2D` commands in
   the app validation barrier. Invalid actions notify visibly. Editing requests focus on
   the first visible shared command input during the following frame.
4. Render the keyboard whenever its user-visible toggle is enabled, in every perspective,
   independent of viewport height.

This is narrower than introducing a Markdown library or a generalized command grammar,
and safer than executing model text speculatively.

## Scope

Included:

- `Sin[x]`, `Cos[t]`, and nested known math-call brackets in expression preprocessing.
- Fast invalid-syntax sampling exit.
- Structural command-card filtering, assistant validation feedback, and edit focus.
- Inline `$$...$$` handling and common LaTeX presentation wrappers.
- Persistent keyboard defaults across perspectives and viewport heights.

Excluded:

- Arbitrary Wolfram Language compatibility.
- Remote tool calls, autonomous model mutation, or external-data commands.
- A full CommonMark or LaTeX engine.

## Technical Design

`grafito-geometry::expr::preprocess_expr` gains a bounded scanner that converts balanced
square brackets to parentheses only after a known built-in expression function name. This
preprocessing already feeds the native AST, the compiled fallback, and GPU preparation,
so one normalization fixes all three paths.

`grafito-core::function_sampling::evaluate_function_samples` returns a minimal all-gap
sample set when both compilation routes fail. It does not bypass sampling for a parsed
expression that evaluates to non-finite values; discontinuity handling stays intact.

`grafito-ui::assistant` filters code fences with a delimiter-balance check, preventing
truncated cards from offering apply/edit controls. Inline text processes `$$` before `$`,
and harmless wrappers such as `\mathbb{R}` render their readable contents. The app retains
the authoritative command validation and adds a focus-pending flag consumed by the shared
command input widget. Invalid candidate actions both set panel state and issue a toast.

`grafito-app::keyboard` removes the viewport-height gate. Every `PerspectiveLayout` sets
`show_math_keyboard: true`; the existing user toggle still permits intentional hiding.

## Acceptance Criteria

1. `Function[Sin[x] / (x^2 + 1)]` samples finite values and produces function geometry.
2. A syntactically invalid function does not spend adaptive-sampling budget and produces
   only gap samples.
3. A truncated `grafito` fence has no apply/edit controls.
4. `Function` and `ParametricCurve2D` assistant proposals with invalid arity are rejected
   visibly before document mutation.
5. Editing a valid assistant proposal selects a visible shared command input.
6. Inline `$$...$$` and `\mathbb{R}` are readable rather than raw or split syntax.
7. The keyboard remains visible at any viewport height and after every perspective switch
   unless the user disables its existing toggle.

## Test Strategy

- Geometry unit tests for call-bracket normalization.
- Core sampling regression tests for the cited function and invalid syntax fast exit.
- Command integration tests for the cited `Function` and assistant-exposed parametric
  arity validation.
- UI unit tests for complete command fences and inline math output.
- App tests for assistant validation, edit-focus state, and all perspective keyboard
  defaults.

## Risks

- Converting all square brackets would corrupt non-call syntax. The scanner is restricted
  to known math function identifiers and balanced groups.
- A generic registry arity rule would incorrectly reject variadic commands. Exact arity is
  initially enforced only for the two command forms currently emitted incorrectly.
- A permanently visible keyboard reduces canvas height. This is the explicit requested
  tradeoff, while the existing user toggle remains available.
