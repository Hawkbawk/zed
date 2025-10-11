# Technical Implementation Plan: Copilot Next Edit Suggestions (NES) Support

**Issue:** https://github.com/zed-industries/zed/issues/30124

**Goal:** Add support for GitHub Copilot's Next Edit Suggestions (NES) feature via the `textDocument/copilotInlineEdit` LSP message to the copilot-language-server.

**Status:** Phase 2 Complete ✅ - See [30124_phase2_summary.md](./30124_phase2_summary.md) for implementation details.

## Background

GitHub Copilot NES is a feature that suggests edits to existing code (not just new code completions). Unlike traditional ghost text completions that only append new text, NES can suggest modifications across multiple locations in a file or even jump to other files.

The key differences from current Zed Edit Predictions (Zeta):
- **NES** is provided by GitHub Copilot via LSP requests
- **Zeta** is Zed's own AI model for edit predictions
- Both should coexist and leverage Zed's existing subtle/eager mode infrastructure

## Current State Analysis

### Edit Prediction Architecture in Zed

1. **Edit Prediction Provider Trait** (`crates/edit_prediction/src/edit_prediction.rs`)
   - Defines the interface all prediction providers must implement
   - Key methods: `refresh()`, `suggest()`, `accept()`, `discard()`, `cycle()`
   - Returns `EditPrediction` enum with two variants:
     - `Local`: edits within the current buffer
     - `Jump`: navigate to a different file

2. **Existing Providers**
   - `ZetaEditPredictionProvider` (Zed's own model)
   - `Zeta2EditPredictionProvider` (newer version)
   - `CopilotCompletionProvider` (current Copilot inline completions)

3. **Subtle vs Eager Mode** (`EditPredictionsMode`)
   - **Subtle**: Preview requires holding modifier key (e.g., Alt/Option)
   - **Eager**: Preview shown automatically when no LSP completions available
   - Controlled via settings: `edit_predictions.mode`
   - Currently applies to all edit prediction providers

4. **Editor Integration**
   - `Editor::update_edit_prediction_preview()` handles modifier key detection
   - `EditPredictionPreview` enum tracks Active/Inactive state
   - Visual indicators: gutter arrows, line highlights, cursor popovers
   - Tab key navigation and acceptance

### Current Copilot Integration

The Copilot integration in Zed already uses `copilot-language-server`:
- File: `crates/copilot/src/copilot.rs`
- Uses LSP requests: `getCompletions`, `getCompletionsCycling`
- Returns traditional inline completions (ghost text)
- Request structure defined in `crates/copilot/src/request.rs`

## Technical Design

### 1. LSP Request/Response Types

**Location:** `crates/copilot/src/request.rs`

Add new types for NES:

```rust
pub enum CopilotInlineEdit {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotInlineEditParams {
    pub text_document: lsp::TextDocumentIdentifier,
    pub position: lsp::Position,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotInlineEditResult {
    pub edits: Vec<CopilotInlineEditItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotInlineEditItem {
    pub text_document: lsp::TextDocumentIdentifier,
    pub range: lsp::Range,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<CopilotEditCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotEditCommand {
    pub title: String,
    pub command: String,
}

impl lsp::request::Request for CopilotInlineEdit {
    type Params = CopilotInlineEditParams;
    type Result = CopilotInlineEditResult;
    const METHOD: &'static str = "textDocument/copilotInlineEdit";
}
```

### 2. Copilot NES Provider

**Location:** Create new file `crates/copilot/src/copilot_nes_provider.rs`

This will be a new edit prediction provider specifically for NES:

```rust
pub struct CopilotNesProvider {
    copilot: Entity<Copilot>,
    project: Entity<Project>,
    pending_refresh: Option<Task<()>>,
    current_edit_suggestion: Option<CurrentNesSuggestion>,
    buffer_id: Option<EntityId>,
}

struct CurrentNesSuggestion {
    buffer_id: EntityId,
    edits: Vec<CopilotInlineEditItem>,
    // Track which edit in the list is currently shown
    active_edit_index: usize,
}

impl EditPredictionProvider for CopilotNesProvider {
    fn name() -> &'static str {
        "copilot-nes"
    }

    fn display_name() -> &'static str {
        "Copilot Next Edit Suggestions"
    }

    fn show_completions_in_menu() -> bool {
        true
    }

    fn show_tab_accept_marker() -> bool {
        true
    }

    fn supports_jump_to_edit() -> bool {
        true  // NES can suggest edits in other files
    }

    // Implementation details below
}
```

Key implementation considerations:

**refresh():**
- Call `textDocument/copilotInlineEdit` LSP request
- Use same throttle/debounce pattern as Zeta (300ms)
- Store all returned edits, not just the first one
- Support multiple pending requests (max 2, like Zeta)

**suggest():**
- Find the edit closest to the cursor position
- Return either:
  - `EditPrediction::Local` for edits in current buffer
  - `EditPrediction::Jump` for edits in different files
- Group nearby edits (within 1 line) together

**cycle():**
- Navigate through multiple edit suggestions
- Update `active_edit_index` to show different edits
- Call `cx.notify()` to trigger re-render

**accept():**
- Apply the currently shown edit(s)
- Notify Copilot via telemetry (may need new LSP method)
- Clear current suggestions

**discard():**
- Clear current suggestions
- Notify Copilot via telemetry

### 3. Copilot API Extensions

**Location:** `crates/copilot/src/copilot.rs`

Add methods to support NES:

```rust
impl Copilot {
    pub fn request_inline_edit(
        &mut self,
        buffer: &Entity<Buffer>,
        position: Anchor,
        cx: &mut Context<Self>,
    ) -> Task<Result<CopilotInlineEditResult>> {
        // Similar pattern to request_completions
        // Get buffer info, convert position to LSP format
        // Make LSP request to copilot-language-server
        // Handle response/errors
    }
}
```

### 4. Provider Registration and Selection

**Location:** Multiple files need updates

The challenge: Users should be able to choose between:
1. Zeta/Zeta2 (Zed's own models)
2. Copilot traditional completions
3. Copilot NES

**Settings Schema** (`crates/settings/src/settings_content/language.rs`):

```rust
pub enum EditPredictionProvider {
    #[serde(rename = "zed")]
    Zed,
    #[serde(rename = "zed2")]
    Zed2,
    #[serde(rename = "copilot")]
    Copilot,
    #[serde(rename = "copilot-nes")]
    CopilotNes,  // NEW
    #[serde(rename = "none")]
    None,
}
```

**Provider Instantiation:**
- Update `Editor::new_provider()` or similar to create `CopilotNesProvider` when selected
- Ensure only one provider is active at a time per editor
- Handle provider switching gracefully

### 5. Subtle Mode Support

**Critical Requirement:** Copilot NES must respect Zed's subtle/eager mode settings.

Based on issue comments:
> "Copilot NES don't support subtle mode, which I REALLY like while using Zed rather than VSCode"

The existing architecture already supports this! No NES-specific changes needed:

**How it works:**
1. Settings determine mode: `edit_predictions.mode` (Subtle or Eager)
2. Editor checks mode in `edit_prediction_settings_at_position()`
3. Sets `preview_requires_modifier` flag
4. Editor's `update_edit_prediction_preview()` monitors modifier keys
5. When modifier held: `EditPredictionPreview::Active`
6. Only renders predictions when active (or in eager mode)

**What Copilot NES Provider needs to do:**
- **Nothing special!** Just implement the standard `EditPredictionProvider` trait
- The editor layer handles all modifier key logic
- Provider's `suggest()` is only called when appropriate

This is a major advantage over VSCode's implementation.

### 6. UI/UX Considerations

**Gutter Indicators:**
- Reuse existing arrow indicators (already implemented)
- Point up/down to nearest edit suggestion
- Show on hover: keyboard shortcuts, settings

**Edit Preview Rendering:**
- Reuse existing diff rendering from Zeta
- Multi-line edit support (already exists)
- Cross-file jump indicators (already exists via `EditPrediction::Jump`)

**Keyboard Navigation:**
- Tab: Navigate to/accept next edit
- Shift+Up/Down: Cycle through multiple suggestions (already mapped for Zeta)
- Escape: Dismiss suggestions

### 7. Testing Strategy

**Unit Tests** (`crates/copilot/src/copilot_nes_provider.rs`):
- Mock LSP responses for various edit scenarios
- Test edit interpolation and cursor proximity logic
- Test cycle() behavior with multiple edits
- Test jump vs local edit detection

**Integration Tests:**
- Test with fake language server returning NES responses
- Verify subtle mode works correctly
- Verify eager mode works correctly
- Test provider switching
- Test multi-file edit suggestions

**Pattern to follow:** See `crates/copilot/src/copilot_completion_provider.rs` tests (lines 270-1128)

## Implementation Phases

### Phase 1: LSP Foundation (1-2 days) ✅ COMPLETE
- [x] Add LSP request/response types to `request.rs`
- [x] Add `request_inline_edit()` method to `Copilot`
- [x] Write unit tests for request/response parsing
- [x] Verify LSP communication with copilot-language-server

### Phase 2: NES Provider (2-3 days) ✅ COMPLETE
- [x] Create `CopilotNesProvider` implementing `EditPredictionProvider`
- [x] Implement `refresh()` with throttling
- [x] Implement `suggest()` with edit selection logic
- [x] Implement `cycle()`, `accept()`, `discard()`
- [x] Write provider unit tests

### Phase 3: Integration (1-2 days)
- [ ] Add `copilot-nes` to `EditPredictionProvider` enum
- [ ] Update provider factory/registration
- [ ] Update settings UI to include Copilot NES option
- [ ] Wire up provider in editor initialization

### Phase 4: Testing & Polish (2-3 days)
- [ ] Integration tests with mock LSP server
- [ ] Manual testing with real copilot-language-server
- [ ] Verify subtle mode behavior matches expectations
- [ ] Verify eager mode behavior
- [ ] Test edge cases (errors, timeouts, empty responses)
- [ ] Performance testing (ensure no LSP request spam)

### Phase 5: Documentation (1 day)
- [ ] Update user documentation
- [ ] Add code comments explaining NES-specific logic
- [ ] Update CHANGELOG

## Edge Cases & Considerations

### 1. Provider Compatibility
**Issue:** User has Copilot NES selected but isn't signed in to Copilot.
**Solution:** Check `Copilot::sign_in_status` in `is_enabled()`, return false if not authorized.

### 2. File Changes
**Issue:** NES suggests edit in file A, but user modified file A before accepting.
**Solution:** Use `edit_preview` mechanism (like Zeta) to validate edits still apply cleanly.

### 3. Multiple Edits
**Issue:** NES returns 5 edits scattered across file.
**Solution:** 
- Show closest edit to cursor by default
- Allow cycling with Shift+Up/Down
- Group edits within 1 line of each other

### 4. Cross-File Edits
**Issue:** NES suggests editing a different file.
**Solution:** 
- Return `EditPrediction::Jump` variant
- Editor already supports this (shows navigation UI)
- After jumping, NES provider should refresh to get edits for new file

### 5. Throttling/Rate Limiting
**Issue:** Too many LSP requests could slow down editor.
**Solution:**
- Use same 300ms throttle as Zeta
- Max 2 pending requests
- Cancel old requests when new one needed

### 6. Copilot vs Copilot NES Overlap
**Issue:** Both provide suggestions, might conflict.
**Solution:**
- Only one edit prediction provider active at a time
- Traditional Copilot completions (ghost text) are separate system
- User chooses: traditional completions OR NES predictions

### 7. LSP Server Version
**Issue:** Older copilot-language-server might not support NES.
**Solution:**
- Check server capabilities on initialization
- Gracefully degrade if `textDocument/copilotInlineEdit` not supported
- Show user-friendly error message

### 8. Telemetry/Analytics
**Issue:** Copilot may want to track NES usage.
**Solution:**
- Check if there are existing LSP methods for telemetry
- May need `notifyNesAccepted`, `notifyNesRejected` (similar to completion telemetry)
- Low priority for initial implementation

## Success Criteria

1. ✅ Users can select "Copilot NES" as edit prediction provider
2. ✅ NES suggestions appear when coding (if enabled)
3. ✅ Subtle mode works: predictions only show when modifier held
4. ✅ Eager mode works: predictions show automatically
5. ✅ Tab key navigates to and accepts suggestions
6. ✅ Shift+Up/Down cycles through multiple suggestions
7. ✅ Cross-file edit suggestions work (jump to other files)
8. ✅ No performance degradation (LSP throttling works)
9. ✅ Error handling is graceful (signed out, server unavailable, etc.)
10. ✅ Coexists peacefully with Zeta and traditional Copilot completions

## Open Questions

1. **Does copilot-language-server require specific version for NES support?**
   - Need to check NPM package versions
   - May need to update minimum version requirement

2. **Are there telemetry/analytics requirements?**
   - GitHub may want usage metrics
   - Check if LSP methods exist

3. **Should there be NES-specific settings?**
   - e.g., disable cross-file suggestions
   - enable/disable gutter indicators
   - Probably not for MVP, use existing settings

4. **How to handle enterprise Copilot configurations?**
   - Some orgs may disable "Editor Preview Features"
   - Need to respect enterprise policies
   - Check with existing Copilot enterprise URI settings

## References

- Issue: https://github.com/zed-industries/zed/issues/30124
- VSCode Blog: https://code.visualstudio.com/blogs/2025/02/12/next-edit-suggestions
- NeoVim Implementation: https://github.com/copilotlsp-nvim/copilot-lsp/blob/main/lua/copilot-lsp/nes/init.lua
- Zed PR switching to copilot-language-server: https://github.com/zed-industries/zed/pull/27401
- Existing Zeta implementation: `crates/zeta/src/zeta.rs`
- Edit prediction trait: `crates/edit_prediction/src/edit_prediction.rs`

## Risk Assessment

**Low Risk:**
- Well-defined LSP protocol
- Existing architecture supports this pattern
- Similar to Zeta implementation (proven approach)
- Subtle mode already implemented

**Medium Risk:**
- LSP server compatibility (version dependencies)
- Multi-file edit UX might need iteration
- Performance with frequent LSP requests

**High Risk:**
- None identified

## Estimated Effort

**Total: 7-11 days** (1.5-2 weeks for one engineer)

- Phase 1: 1-2 days
- Phase 2: 2-3 days  
- Phase 3: 1-2 days
- Phase 4: 2-3 days
- Phase 5: 1 day

This assumes:
- Familiarity with Zed codebase
- No major architecture changes needed
- copilot-language-server supports the feature
- No unexpected LSP protocol issues