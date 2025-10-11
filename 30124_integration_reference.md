# Copilot NES Integration Reference

This document provides a quick reference for developers working with the Copilot Next Edit Suggestions (NES) integration in Zed.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         User Settings                            │
│  { "features": { "edit_prediction_provider": "copilot_nes" } }  │
└─────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│              EditPredictionProvider Enum (Settings)              │
│  None | Copilot | CopilotNes | Supermaven | Zed                 │
└─────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│            Edit Prediction Registry (Provider Factory)          │
│  - Observes settings changes                                    │
│  - Creates appropriate provider instance                        │
│  - Assigns to editor                                            │
└─────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                    CopilotNesProvider                            │
│  - Implements EditPredictionProvider trait                      │
│  - Communicates with copilot-language-server via LSP            │
│  - Manages edit suggestions and cycling                         │
└─────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                   Copilot (LSP Client)                           │
│  - request_inline_edit() sends textDocument/copilotInlineEdit   │
│  - Handles authentication and buffer registration               │
└─────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                  copilot-language-server                         │
│  - Returns CopilotInlineEditResult                              │
│  - Contains array of CopilotInlineEditItem                      │
└─────────────────────────────────────────────────────────────────┘
```

## Key Files

### Core Implementation
- `crates/copilot/src/copilot_nes_provider.rs` - Main provider implementation
- `crates/copilot/src/request.rs` - LSP request/response types
- `crates/copilot/src/copilot.rs` - Copilot client with `request_inline_edit()`

### Settings & Configuration
- `crates/settings/src/settings_content/language.rs` - EditPredictionProvider enum

### Integration
- `crates/zed/src/zed/edit_prediction_registry.rs` - Provider factory and registration
- `crates/edit_prediction_button/src/edit_prediction_button.rs` - UI button and menus

### Trait Definition
- `crates/edit_prediction/src/edit_prediction.rs` - EditPredictionProvider trait

## Provider Lifecycle

### 1. Initialization
```rust
// When user selects "copilot_nes" in settings:
EditPredictionProvider::CopilotNes => {
    if let Some(copilot) = Copilot::global(cx) {
        // Register buffer with Copilot for LSP communication
        copilot.update(cx, |copilot, cx| {
            copilot.register_buffer(&buffer, cx);
        });
        
        // Create provider instance
        let provider = cx.new(|_| CopilotNesProvider::new(copilot, project.clone()));
        
        // Assign to editor
        editor.set_edit_prediction_provider(Some(provider), window, cx);
    }
}
```

### 2. Requesting Suggestions
```rust
// When user types, refresh() is called with throttling
fn refresh(&mut self, buffer: Entity<Buffer>, cursor: Anchor, cx: &mut Context<Self>) {
    // Throttle requests (300ms)
    // Max 2 pending requests
    // Call copilot.request_inline_edit()
}
```

### 3. Displaying Suggestions
```rust
// Editor calls suggest() to get current suggestion
fn suggest(&self, buffer: &Entity<Buffer>, cursor: Anchor, cx: &App) -> Option<EditPrediction> {
    // Find closest edit to cursor
    // Return EditPrediction::Local or EditPrediction::Jump
}
```

### 4. User Interaction
- **Tab**: Navigate to/accept suggestion
- **Shift+Up/Down**: Cycle through suggestions
- **Escape**: Discard suggestions
- **Modifier+Hold** (Subtle mode): Show preview

## Data Flow

### LSP Request
```rust
CopilotInlineEditParams {
    text_document: lsp::TextDocumentIdentifier,
    position: lsp::Position,
    version: Option<usize>,
}
```

### LSP Response
```rust
CopilotInlineEditResult {
    edits: Vec<CopilotInlineEditItem>,
}

CopilotInlineEditItem {
    text_document: lsp::TextDocumentIdentifier,
    range: lsp::Range,           // UTF-16 based
    text: String,                // Original text in range
    new_text: Option<String>,    // Replacement text
    command: Option<CopilotEditCommand>,
}
```

### Internal Representation
```rust
EditPrediction::Local {
    buffer: Entity<Buffer>,
    edits: Vec<(Range<Anchor>, String)>,  // Converted to byte-based anchors
    show_cursor_popover_hint: bool,
}
```

## Important Conversions

### LSP Range → Zed Range
```rust
// LSP uses UTF-16 code units
let lsp_range = item.range;

// Convert to Zed's byte-based points
let start = point_from_lsp(lsp_range.start);
let end = point_from_lsp(lsp_range.end);

// Convert to anchors (survive buffer edits)
let start_anchor = buffer.read(cx).anchor_before(start);
let end_anchor = buffer.read(cx).anchor_after(end);
```

### Grouping Nearby Edits
```rust
// Edits within 1 line are grouped together
if edits are on same or adjacent lines {
    return as single EditPrediction::Local
}
```

## Settings Integration

### User-Facing Setting Names
```json
{
  "features": {
    "edit_prediction_provider": "copilot_nes"  // snake_case serialization
  }
}
```

### Available Providers
- `"none"` → `EditPredictionProvider::None`
- `"copilot"` → `EditPredictionProvider::Copilot`
- `"copilot_nes"` → `EditPredictionProvider::CopilotNes`
- `"supermaven"` → `EditPredictionProvider::Supermaven`
- `"zed"` → `EditPredictionProvider::Zed`

### Mode Settings (Apply to All Providers)
```json
{
  "edit_predictions": {
    "mode": "subtle",  // or "eager"
  }
}
```

## UI Components

### Status Bar Button
- Both `Copilot` and `CopilotNes` share the same Copilot icon
- Shows authentication status
- Opens context menu on click

### Context Menu (when Copilot is active)
```
┌────────────────────────────────────────┐
│ Show Edit Predictions For              │
│   ☑ This Buffer                        │
│   ☑ [Language Name]                    │
│   ☑ All Files                          │
├────────────────────────────────────────┤
│ Display Modes                          │
│   ○ Eager                              │
│   ● Subtle                             │
├────────────────────────────────────────┤
│ Use Copilot Next Edit Suggestions      │  ← Only shown if on Copilot
│   OR                                   │
│ Use Copilot Completions                │  ← Only shown if on CopilotNes
├────────────────────────────────────────┤
│ Use Zed AI instead                     │
├────────────────────────────────────────┤
│ Go to Copilot Settings                 │
│ Sign Out                               │
└────────────────────────────────────────┘
```

## Testing Considerations

### Unit Tests
- Mock LSP responses in `copilot_nes_provider.rs`
- Test edit interpolation and cursor proximity
- Test cycle() with multiple edits
- Test throttling behavior

### Integration Tests
- Use `EditorLspTestContext` for simulating LSP
- Test provider switching
- Test subtle/eager modes
- Verify edit application and undo

### Pattern to Follow
```rust
#[gpui::test]
async fn test_copilot_nes_provider(cx: &mut TestAppContext) {
    let (copilot, copilot_lsp) = Copilot::fake(cx);
    let provider = cx.new(|_| CopilotNesProvider::new(copilot, project));
    
    // Mock LSP response
    copilot_lsp.handle_request::<CopilotInlineEdit, _, _>(|params, _| async move {
        Ok(CopilotInlineEditResult { edits: vec![...] })
    });
    
    // Test provider behavior
    provider.update(cx, |provider, cx| {
        provider.refresh(buffer, cursor, cx);
    });
}
```

## Common Pitfalls

### 1. UTF-16 vs Byte-Based Points
❌ **Wrong:** Using LSP ranges directly
```rust
let range = item.range;  // This is UTF-16!
```

✅ **Correct:** Convert via point_from_lsp
```rust
let start = point_from_lsp(item.range.start);
let end = point_from_lsp(item.range.end);
```

### 2. Entity Cloning
❌ **Wrong:** Passing entity references
```rust
CopilotNesProvider::new(copilot, project)  // &Entity
```

✅ **Correct:** Clone entities
```rust
CopilotNesProvider::new(copilot, project.clone())
```

### 3. Throttling
❌ **Wrong:** Making LSP request on every keystroke
```rust
fn refresh(...) {
    self.request_edit(...)  // Spams LSP server!
}
```

✅ **Correct:** Throttle and limit pending requests
```rust
fn refresh(...) {
    if elapsed < THROTTLE_TIMEOUT { return; }
    if pending_predictions.len() >= 2 { return; }
    // ... make request
}
```

### 4. Error Handling
❌ **Wrong:** Silently ignoring errors
```rust
let _ = copilot.request_inline_edit(...);  // Discards error!
```

✅ **Correct:** Log and handle errors
```rust
copilot.request_inline_edit(...)
    .await
    .log_err();
```

## Extension Points (Future Work)

### Cross-File Edits (Phase 4+)
```rust
// When edit is in different file:
EditPrediction::Jump {
    buffer: Entity<Buffer>,     // Target buffer
    position: Anchor,           // Where to jump
    label: String,              // "Edit in other_file.rs"
}
```

### Telemetry (Phase 5)
```rust
// Track accept/reject for analytics
fn accept(...) {
    // Apply edits
    copilot.notify_nes_accepted(...);  // Future LSP method
}

fn discard(...) {
    copilot.notify_nes_rejected(...);  // Future LSP method
}
```

## Debugging

### Enable Logging
```bash
RUST_LOG=copilot=debug,edit_prediction=debug zed
```

### Check LSP Communication
```rust
// Add logging in request_inline_edit()
log::debug!("Requesting NES for position: {:?}", position);
```

### Verify Provider Selection
```rust
// In edit_prediction_registry.rs
log::info!("Assigning provider: {:?}", provider);
```

## References

- [Technical Plan](./30124_tech_plan.md)
- [Phase 1 Summary](./30124_phase1_summary.md) - LSP Foundation
- [Phase 2 Summary](./30124_phase2_summary.md) - Provider Implementation
- [Phase 3 Summary](./30124_phase3_summary.md) - Integration
- [GitHub Issue #30124](https://github.com/zed-industries/zed/issues/30124)
- [EditPredictionProvider Trait](../crates/edit_prediction/src/edit_prediction.rs)