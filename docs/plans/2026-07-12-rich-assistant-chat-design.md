# Rich Assistant Chat Design

## Problem

The assistant panel behaves like a compact form: the composer consumes the top
of the panel, answers render as raw Markdown, and `minimax-m3` rejects image
attachments even though its Anthropic Messages transport can represent them.
This prevents mathematical responses, tables and Grafito guidance from being
useful at the point of work.

## Reframe

The assistant must be a safe math conversation surface, not a second command
executor. It should make long answers scannable, keep the next message ready
at the bottom, support explicitly consented visual questions with Minimax, and
let the user review a prepared Grafito command before they execute it.

## Approach

Use a compact, dependency-free renderer for the response subset the assistant
is instructed to emit: headings, bullets, code, fenced `grafito` commands,
pipe tables and inline/display LaTex. LaTex is preserved in equation cards and
normalized into a readable mathematical representation; full TeX typesetting
is deliberately deferred because no compatible renderer is already present.
The remote prompt permits one fenced, documented Grafito command suggestion.
The UI can insert a strictly validated suggestion into Grafito's existing
command bar but never executes it.

## Scope

In scope:

1. Scrollable transcript with a fixed bottom composer, rounded message cards
   and existing vector action icons.
2. Safe Markdown subset rendering for assistant turns, including native tables
   and readable LaTex equation cards.
3. User-reviewed handoff of allowlisted Grafito commands to the existing input
   bar.
4. Validated PNG/JPEG Anthropic image blocks for explicitly enabled
   `minimax-m3`; Fusion remains text-only.

Out of scope:

1. Arbitrary remote command execution, scripts, files, exports or mutation of
   the document.
2. Full TeX layout engine, streaming, OCR, or automatic model capability
   discovery.
3. Image input for Fusion because the second DeepSeek audit leg is text-only.

## Technical Design

`grafito-ui::assistant` splits the panel into header, transcript and composer.
The transcript uses `ScrollArea::stick_to_bottom(true)` and assistant turns use
a bounded line parser. Only completed `grafito` fences result in a candidate
button. The app validates candidates as one single-line command registered in
Grafito's command registry, excluding scripts and external-data commands,
before assigning them to `GrafitoApp::input_text`.

`build_anthropic_messages_payload` validates decoded attachments, capability
and consent before placing text plus `image`/`source` base64 blocks into the
last user Anthropic message. It rejects Fusion before its draft request, so no
image reaches the text-only audit pipeline.

The transcript bubble uses an outer horizontal alignment only to choose the
user/assistant side; its frame always resets to a vertical content layout. The
composer is intentionally compact: two editor rows, a 160-character focus
preview and request limits on hover. At widths below 1120 px the desktop shell
keeps only the permanent assistant drawer; at heights below 760 px it preserves
the keyboard preference but does not allocate the 180 px on-screen keyboard.

## Acceptance Criteria

1. The transcript scrolls independently and new replies keep the composer
   visible at the bottom of the assistant panel.
2. Assistant output renders headings, bullets, code, valid Markdown tables,
   inline/display LaTex and safe command cards without raw Markdown markers.
3. Only complete, one-line `grafito` fences containing registered non-I/O
   commands may offer insertion; inserting neither executes a command nor
   mutates the document.
4. `minimax-m3` accepts validated PNG/JPEG attachments only after explicit
   vision acknowledgement and per-request upload consent. Fusion rejects them.
5. Remote payloads never contain paths, names, transcriptions, API keys, or
   OpenAI data URLs for Anthropic image blocks.
6. A 960 px viewport keeps the assistant and bottom command input but hides the
   rail/left drawer, while a 600 px height hides the mathematical keyboard.

## Test Strategy

Unit-test parser classification, fenced-command extraction, command handoff
validation and Minimax payload shape/rejections. Run targeted UI/app/transport
tests followed by workspace formatting, clippy, tests, release build and
package installation checks.

## Risks

1. A subset renderer is not a full TeX engine. Equation cards retain exact
   source and normalize common commands so correctness is never hidden.
2. Remote suggestions are untrusted. Strict parsing, an allowlist and insertion
   without execution prevent response text from mutating a document.
3. Provider support can change. Both vision controls remain explicit and
   transport validation remains authoritative.
