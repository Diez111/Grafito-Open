# Assistant UI Recovery Design

## Problem

The permanent assistant panel makes the user type a model identifier, repeats explanatory copy, mixes connection/import state, and leaves image consent visually disconnected from the attachment it authorizes. The result is a dense configuration form instead of a focused conversation surface.

## Reframe

The assistant should make the next valid action obvious: choose a provider and model from a controlled list, write a question, attach an image only when needed, then review a concise consent block. It must disclose actual request limits but must not fabricate remaining OpenCode quota.

## Approach

Use a compact chat-first layout. A single model `ComboBox` merges provider defaults and dynamically discovered models. The connection details use a compact card, while focus, attachments and consent appear only when relevant. This is the balanced approach: it removes invalid free-form model entry without adding a settings window.

## Scope

In scope:

1. Provider-specific static model catalogs plus deduplicated remotely discovered models.
2. Model selection only through `egui::ComboBox`.
3. Compact hierarchy, Spanish copy, design tokens and conditional guidance.
4. Attachment status separated from connection status; visible request limits and retained explicit vision/upload consent.
5. UI state tests for model choices, consent gating and request behavior.

Out of scope:

1. Live remaining-quota reporting, streaming, or server-side model capability discovery.
2. Automatically inferring vision support from a model name or silently uploading attachments. `minimax-m3` and Fusion are explicitly text-only until their end-to-end vision support is documented.

## Technical Design

`AssistantPanelState` exposes `model_choices()` from provider defaults, the selected model and `available_models`; `select_model()` remains the only mutation route and resets image-upload consent. `can_submit()` permits a request with a saved-but-not-yet-observed key so app integration can resolve it from the keyring on demand; for attachments, both vision acknowledgement and image-upload consent remain mandatory. The app continues enforcing these checks independently before a worker is started.

`draw_panel_contents()` keeps only a title with a settings affordance, question editor with suggestions only while empty, conditional attachment/consent cards, and the conversation region. Provider/model/key controls live in a separate configuration dialog; provider and model persist as non-secret `AppConfig` preferences, while keys live in the system credential store. Limits use `RequestBudget::default()` and `AttachmentLimits::default()` values rather than a hardcoded quota claim. Fusion runs Minimax M3 through OpenCode's Anthropic Messages endpoint, then sends its bounded draft and retained conversation to DeepSeek v4 Pro for an audit; the draft is discarded if the audit cannot complete.

## Acceptance Criteria

1. The model cannot be edited as text; a ComboBox includes all known OpenCode Go models, the Ollama defaults, discovered models and the active selection.
2. The submit action is disabled if attachments lack either explicit vision acknowledgement or per-request upload consent; app/transport validation remains unchanged.
3. The empty-focus prompt, unused vision checkbox and repetitive instructions are absent from the initial panel.
4. Connection and import status are shown in their relevant sections; limits disclose 4 KiB input, 4 KiB response, 15 seconds, and image count/size/pixel caps.
5. Model, provider or attachment changes reset image-upload consent.

## Test Strategy

Add state tests for stable model catalog/deduplication, consent-aware submission and model-change reset. Extend source-level UI regression tests to require the model ComboBox and Spanish attachment/focus labels while forbidding free-form model input. Run UI/app/assistant targeted tests followed by workspace formatting, clippy, tests and release build.

## Risks

1. Static catalog models may change upstream; the refresh action appends the authenticated `/models` response without opening free-form entry.
2. Some listed models may not support vision; the current explicit acknowledgement remains mandatory and the transport rejects absent consent.
3. A narrow side panel can overflow verbose status copy; all persistent labels stay short and secondary details use small text or conditional display.
