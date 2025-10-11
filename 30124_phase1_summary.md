# Phase 1 Implementation Summary: Copilot NES LSP Foundation

**Date:** 2025-01-XX  
**Issue:** https://github.com/zed-industries/zed/issues/30124  
**Tech Plan:** [30124_tech_plan.md](./30124_tech_plan.md)

## Overview

Phase 1 of the Copilot Next Edit Suggestions (NES) implementation is now complete. This phase focused on establishing the LSP foundation for communicating with the copilot-language-server to request inline edit suggestions.

## What Was Implemented

### 1. LSP Request/Response Types (`crates/copilot/src/request.rs`)

Added the following types to support the `textDocument/copilotInlineEdit` LSP request:

- **`CopilotInlineEdit`**: Empty enum implementing `lsp::request::Request` trait
  - Method: `"textDocument/copilotInlineEdit"`
  - Params: `CopilotInlineEditParams`
  - Result: `CopilotInlineEditResult`

- **`CopilotInlineEditParams`**: Request parameters
  - `text_document`: LSP text document identifier
  - `position`: Cursor position in the document
  - `version`: Optional document version number

- **`CopilotInlineEditResult`**: Response structure
  - `edits`: Vector of `CopilotInlineEditItem`

- **`CopilotInlineEditItem`**: Individual edit suggestion
  - `text_document`: Document identifier (supports cross-file edits)
  - `range`: Range of text to replace
  - `text`: Original text at the range
  - `new_text`: Optional replacement text
  - `command`: Optional command to execute

- **`CopilotEditCommand`**: Command metadata
  - `title`: User-facing command title
  - `command`: Command identifier

All types implement:
- `Debug`, `Clone`, `Serialize`, `Deserialize` (required for LSP)
- `PartialEq`, `Eq` (for testing)

### 2. Copilot API Extension (`crates/copilot/src/copilot.rs`)

Added `request_inline_edit()` method to the `Copilot` struct:

```rust
pub fn request_inline_edit<T>(
    &mut self,
    buffer: &Entity<Buffer>,
    position: T,
    cx: &mut Context<Self>,
) -> Task<Result<request::CopilotInlineEditResult>>
where
    T: ToPointUtf16,
```

**Implementation Details:**
- Registers buffer with Copilot server if not already registered
- Authenticates with Copilot server (returns error if not signed in)
- Converts position to LSP format (UTF-16)
- Makes async LSP request to copilot-language-server
- Returns task that resolves to result or error

**Follows Same Pattern As:**
- `request_completions()` for traditional ghost text completions
- Reuses buffer registration and authentication logic
- Uses background spawning for async LSP communication

### 3. Comprehensive Tests

#### Test 1: `test_inline_edit_request` (Integration Test)
- Creates fake Copilot server and buffer
- Registers buffer and waits for `DidOpenTextDocument` notification
- Sets up request handler to respond with mock NES data
- Makes inline edit request at position (0, 0)
- Verifies response structure and content
- **Status:** ✅ Passing

#### Test 2: `test_inline_edit_serde` (Unit Test)
- Tests JSON serialization of request parameters
- Tests JSON deserialization of response with all fields
- Tests JSON deserialization with optional fields missing
- Validates camelCase field name conversion
- **Status:** ✅ Passing

### 4. Quality Assurance

All quality checks pass:
- ✅ `cargo test -p copilot` - All 11 tests passing
- ✅ `cargo clippy -p copilot` - No warnings
- ✅ `cargo check -p copilot` - Compilation successful
- ✅ `cargo build -p copilot --release` - Release build successful
- ✅ No diagnostics errors or warnings

## Files Modified

1. `zed/crates/copilot/src/request.rs`
   - Added 45 lines for NES types
   - Lines 227-268

2. `zed/crates/copilot/src/copilot.rs`
   - Added `request_inline_edit()` method (40 lines)
   - Added integration test (45 lines)
   - Added serde test (55 lines)
   - Added `futures::StreamExt` import for tests
   - Total additions: ~140 lines

## Technical Decisions

### 1. Optional Fields
Made `new_text`, `command`, and `version` optional to handle various response formats from copilot-language-server. This provides flexibility for:
- Edits that only delete (no new_text)
- Simple edits without commands
- Requests without version tracking

### 2. Cross-File Support
`CopilotInlineEditItem` includes `text_document` field to support cross-file edit suggestions. This aligns with the NES capability to suggest edits in other files.

### 3. Derivable Traits
Added `PartialEq` and `Eq` to support test assertions with `assert_eq!`. This is common practice for test-friendly types.

### 4. Error Handling
Follows Zed's pattern of propagating errors with `?` and providing context with `.context()`. No silent error discarding.

## Phase 1 Checklist Status

- ✅ Add LSP request/response types to `request.rs`
- ✅ Add `request_inline_edit()` method to `Copilot`
- ✅ Write unit tests for request/response parsing
- ✅ Verify LSP communication with copilot-language-server (via fake server)

## Next Steps (Phase 2)

Phase 2 will implement the actual edit prediction provider:

1. Create `CopilotNesProvider` struct implementing `EditPredictionProvider` trait
2. Implement `refresh()` to call `request_inline_edit()` with throttling
3. Implement `suggest()` to return edit suggestions near cursor
4. Implement `cycle()` to navigate between multiple suggestions
5. Implement `accept()` and `discard()` for telemetry
6. Write comprehensive provider tests

## Dependencies for Phase 2

Phase 2 will need:
- `EditPredictionProvider` trait from `crates/edit_prediction`
- `EditPrediction` enum (Local vs Jump variants)
- Anchor/Range types from `language` crate
- Throttling/debouncing utilities (300ms delay like Zeta)

## Notes

- The LSP method `textDocument/copilotInlineEdit` may require a specific version of copilot-language-server. This should be verified during Phase 4 (Testing & Polish).
- No telemetry methods have been identified yet for NES accept/reject. This may be discovered in Phase 2 or 4.
- The implementation follows Zed's coding guidelines: no unwrap(), proper error propagation, full variable names.

## Testing Instructions

To run the new tests:

```bash
# All copilot tests
cargo test -p copilot

# Just the NES tests
cargo test -p copilot test_inline_edit

# With verbose output
cargo test -p copilot test_inline_edit -- --nocapture
```

## References

- LSP Specification: https://microsoft.github.io/language-server-protocol/
- NeoVim NES Implementation: https://github.com/copilotlsp-nvim/copilot-lsp/blob/main/lua/copilot-lsp/nes/init.lua
- Existing Copilot requests: `crates/copilot/src/request.rs` lines 1-225
- Existing request pattern: `Copilot::request_completions()` at line 933