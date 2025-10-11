# Phase 2 Implementation Summary: Copilot NES Provider

**Date:** 2025-01-XX
**Phase:** 2 of 5
**Status:** ✅ Complete

## Overview

Phase 2 successfully implemented the `CopilotNesProvider` as an `EditPredictionProvider`, enabling Zed to receive and manage Next Edit Suggestions from GitHub Copilot's language server.

## What Was Implemented

### 1. CopilotNesProvider Implementation (`crates/copilot/src/copilot_nes_provider.rs`)

Created a new edit prediction provider implementing the `EditPredictionProvider` trait with the following features:

**Core Structure:**
- `CopilotNesProvider` struct with fields:
  - `copilot: Entity<Copilot>` - Reference to Copilot service
  - `project: Entity<Project>` - Reference to project (for future cross-file jumps)
  - `buffer_id: Option<EntityId>` - Tracks which buffer edits are for
  - `edits: Vec<CopilotInlineEditItem>` - Current edit suggestions from LSP
  - `active_edit_index: usize` - Which edit is currently shown
  - `next_pending_prediction_id: usize` - For tracking pending requests
  - `pending_predictions: Vec<PendingPrediction>` - Active LSP requests
  - `last_request_timestamp: Instant` - For throttling

**Key Methods Implemented:**

1. **`refresh()`** - Requests new edit suggestions from Copilot
   - Implements 300ms throttle to avoid spamming LSP server
   - Maintains max 2 pending requests (cancels old ones)
   - Calls `copilot.request_inline_edit()` via async spawn
   - Stores returned edits for later cycling

2. **`suggest()`** - Returns the edit suggestion to display
   - Validates edits are still applicable to current buffer
   - Finds edit closest to cursor position
   - Groups nearby edits (within 1 line) together
   - Converts LSP ranges to Zed anchors
   - Returns `EditPrediction::Local` with edit ranges and text
   - Handles cross-file edits by returning `None` (TODO for Phase 3)

3. **`cycle()`** - Navigate between multiple suggestions
   - Supports `Direction::Next` and `Direction::Prev`
   - Wraps around when reaching end/beginning
   - Calls `cx.notify()` to trigger re-render

4. **`accept()`** - User accepted the suggestion
   - Clears current suggestions
   - TODO: Add telemetry notification to Copilot

5. **`discard()`** - User dismissed the suggestion
   - Clears current suggestions
   - TODO: Add telemetry notification to Copilot

6. **`is_enabled()`** - Checks if provider should be active
   - Returns true only if user is signed in to Copilot

7. **`is_refreshing()`** - UI indicator for loading state
   - Returns true when pending predictions exist

**Trait Methods:**
- `name()` → `"copilot-nes"`
- `display_name()` → `"Copilot Next Edit Suggestions"`
- `show_completions_in_menu()` → `true`
- `show_tab_accept_marker()` → `true`
- `supports_jump_to_edit()` → `true` (for future cross-file support)

### 2. Module Export

Updated `crates/copilot/src/copilot.rs`:
- Added `mod copilot_nes_provider;`
- Added `pub use crate::copilot_nes_provider::CopilotNesProvider;`

### 3. Tests

Implemented two test cases:

**`test_copilot_nes_cycle`**
- Tests cycling through multiple edit suggestions
- Verifies forward cycling with wrap-around
- Verifies backward cycling

**`test_copilot_nes_accept_discard`**
- Tests accept() clears state
- Tests discard() clears state
- Ensures buffer_id and edits are reset

Both tests follow the established pattern from `copilot_completion_provider.rs` tests.

## Technical Decisions

### 1. Throttling Strategy
- Used 300ms throttle (same as Zeta2) to balance responsiveness and LSP load
- Maintain max 2 pending requests to allow rapid cursor movement

### 2. Edit Selection Logic
- Find closest edit to cursor by comparing row distances
- Group edits within 1 line of each other to show related changes together
- This matches Zeta2's grouping behavior

### 3. Point Conversion
- LSP returns `PointUtf16` (UTF-16 code units)
- Convert to `Point` (byte offsets) using `buffer.unclipped_point_utf16_to_point()`
- Critical for proper edit range calculation

### 4. URI Handling
- For local files: use `file.abs_path(cx)` with `lsp::Uri::from_file_path()`
- For non-file buffers: use `buffer://{entity_id}` format
- Ensures edits match the correct buffer

### 5. Cross-File Edits
- Detected when `edit.text_document.uri != buffer_uri`
- Currently returns `None` (not implemented)
- TODO: Need to implement `EditPrediction::Jump` support in Phase 3

## Deferred Items (TODOs)

1. **Telemetry** - Add notifications to Copilot when edits are accepted/rejected
   - May need new LSP methods like `notifyNesAccepted`, `notifyNesRejected`
   - Similar to existing completion telemetry

2. **Cross-File Jumps** - Return `EditPrediction::Jump` for edits in other files
   - Need to find/load target buffer in project
   - Create snapshot and anchor for jump target
   - UI will handle showing navigation prompt

3. **Edit Preview** - Add `edit_preview` field to `EditPrediction::Local`
   - Allows validation that edits still apply cleanly
   - Zeta providers use this for detecting conflicts

## Testing Results

✅ All tests pass:
```
test copilot_nes_provider::tests::test_copilot_nes_cycle ... ok
test copilot_nes_provider::tests::test_copilot_nes_accept_discard ... ok
```

✅ Clippy clean (except expected warning about unused `project` field)

✅ Code compiles successfully

## Files Changed

- `crates/copilot/src/copilot_nes_provider.rs` - **NEW** (495 lines)
- `crates/copilot/src/copilot.rs` - Added module declaration and export

## Next Steps (Phase 3)

Phase 3 will integrate the provider into the editor:

1. Add `copilot-nes` to `EditPredictionProvider` enum in settings
2. Update provider factory/registration in editor
3. Update settings UI to show Copilot NES option
4. Wire up provider in editor initialization
5. Ensure only one provider is active at a time

See [30124_tech_plan.md](./30124_tech_plan.md) for full Phase 3 requirements.

## Notes

- The provider respects Zed's existing subtle/eager mode infrastructure automatically
- No NES-specific code needed for modifier key handling
- UI rendering (gutter arrows, diff previews) already implemented in editor layer
- Keyboard shortcuts (Tab, Shift+Up/Down) already mapped to provider methods

## References

- Phase 1 Summary: [30124_phase1_summary.md](./30124_phase1_summary.md)
- Tech Plan: [30124_tech_plan.md](./30124_tech_plan.md)
- Issue: https://github.com/zed-industries/zed/issues/30124