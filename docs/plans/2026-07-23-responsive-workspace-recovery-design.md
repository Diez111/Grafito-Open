# Responsive Workspace Recovery Design

## Problem

The supplied desktop captures show that fixed panels consume workspace even
when they are not actively needed: the virtual keyboard reserves a 260 px
bottom strip, the assistant cannot be hidden, and the left workspace drawer
appears abruptly at a width that leaves too little canvas. On shorter windows,
these independent reservations can leave no usable canvas.

## Reframe

The goal is not to remove mathematical controls. Grafito must keep the
keyboard, assistant, command entry, and workspace panels reachable while
allocating persistent screen area only to the controls currently needed.

## Approach

Use a small, deterministic responsive policy instead of a visual redesign:

- Render the keyboard as a compact quick-key strip below the full-keyboard
  height threshold, with an explicit expand action.
- Delay the persistent left drawer until it can coexist with the assistant and
  a useful canvas; keep the rail available to reopen it.
- Make the assistant hideable and keep its default composer compact.
- Reuse the same keyboard layout decision for panel reservation and rendering.
- Repair the domain-coloring drawer so idle frames do not mutate document
  revision state.

## Scope

In scope: `grafito-app` shell, keyboard, panel mutation paths, and
`grafito-ui` assistant controls.

Out of scope: a new retained-mode layout framework, removal of existing
perspectives, changing command semantics, or a wholesale visual theme rewrite.

## Technical Design

`keyboard.rs` will expose a pure `MathKeyboardLayout` decision from visibility,
explicit expansion, and viewport height. `app.rs` will reserve exactly the
height returned by that decision and pass it to the renderer. The compact form
keeps primary insertion, delete, submit, and expand controls available.

`ShellLayout` will accept whether the left drawer is open, retain the rail when
the drawer is closed, and recompute bottom-entry visibility after a compact
drawer is opened. The medium breakpoint moves to the width at which the rail,
default left drawer, assistant, and a 760 px canvas fit together.

The assistant header will emit an explicit hide action. The application keeps
polling assistant jobs while hidden so requests cannot become stale merely
because the panel is closed. The hidden panel can be restored from the existing
Herramientas menu.

The domain-coloring controls will read immutable object state, collect the
user edit locally, and acquire mutable document access only if the value
actually changed.

## Acceptance Criteria

1. A visible keyboard uses the compact strip below 760 px height unless the
   user explicitly expands it; reservation and rendered layout always match.
2. Compact Algebra/CAS drawers show one command entry, not both the embedded
   editor and bottom bar.
3. At 1280 px the shell remains canvas-focused; at 1360 px it may show the
   rail and left drawer while retaining the assistant and at least 760 px of
   default canvas width.
4. The assistant can be hidden and restored without canceling or losing a
   pending request.
5. An idle Domain Coloring drawer does not change `Document::version`; a real
   setting change increments it once.
6. Strict workspace Clippy, tests, release build, and formatting pass.

## Test Strategy

Unit-test the pure keyboard and shell policies at breakpoint boundaries. Test
the panel mutation helpers with unchanged and changed complex objects. Keep
existing assistant composer tests, adding an explicit compact-baseline guard.
Run focused crate tests first, then workspace format, Clippy, tests, release
build, and `git diff --check`.

## Risks

- A compact keyboard must remain discoverable and usable; it therefore retains
  quick keys, submit/delete, and a labeled expand affordance.
- A hidden assistant must continue reaping remote jobs; polling stays outside
  the visibility branch.
- Existing eframe panel-size memory can persist user-selected widths, so the
  drawer remains resizable rather than forcibly resetting a user choice.
