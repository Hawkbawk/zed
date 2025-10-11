# Phase 3 Manual Testing Guide: Copilot NES Integration

This guide helps you manually test the Copilot NES integration in Zed.

## Prerequisites

Before testing, ensure:
- [ ] Zed is built with the Phase 3 changes
- [ ] You have a GitHub Copilot subscription
- [ ] You're signed into Copilot in Zed
- [ ] `copilot-language-server` supports `textDocument/copilotInlineEdit` (version check may be needed)

## Test Plan

### 1. Settings Configuration

#### Test 1.1: Enable via Settings File
1. Open Zed settings (`Cmd+,` or `Zed > Settings`)
2. Add the following to your settings:
   ```json
   {
     "features": {
       "edit_prediction_provider": "copilot_nes"
     }
   }
   ```
3. Save the settings file
4. **Expected:** No errors, settings are applied

#### Test 1.2: Verify Provider Change
1. Open any code file
2. Check the status bar (bottom right)
3. **Expected:** Copilot icon is visible
4. Click the Copilot icon
5. **Expected:** Context menu shows "Use Copilot Completions" option (since we're on NES)

#### Test 1.3: Switch Back to Traditional Copilot
1. In the Copilot context menu, click "Use Copilot Completions"
2. **Expected:** Menu closes, provider switches to traditional Copilot
3. Open the menu again
4. **Expected:** Menu now shows "Use Copilot Next Edit Suggestions" option

### 2. Edit Predictions - Subtle Mode

#### Test 2.1: Enable Subtle Mode
1. Ensure settings have:
   ```json
   {
     "features": {
       "edit_prediction_provider": "copilot_nes"
     },
     "edit_predictions": {
       "mode": "subtle"
     }
   }
   ```
2. Open a JavaScript/TypeScript file
3. Start typing a function

#### Test 2.2: Verify Modifier Key Behavior
1. Type some code and pause
2. **Expected:** No preview appears automatically
3. Hold the Alt/Option key (default modifier)
4. **Expected:** Edit prediction preview appears (if available)
5. Release the modifier key
6. **Expected:** Preview disappears

#### Test 2.3: Accept Prediction
1. Hold modifier key to show preview
2. Press Tab
3. **Expected:** Edit is applied to the buffer
4. **Expected:** Cursor moves to the edited location

### 3. Edit Predictions - Eager Mode

#### Test 3.1: Enable Eager Mode
1. Update settings:
   ```json
   {
     "features": {
       "edit_prediction_provider": "copilot_nes"
     },
     "edit_predictions": {
       "mode": "eager"
     }
   }
   ```
2. Open a code file
3. Start typing

#### Test 3.2: Verify Automatic Display
1. Type some code and pause
2. **Expected:** Preview appears automatically (without holding modifier)
3. **Expected:** Preview shows inline with diff highlighting

#### Test 3.3: Discard Prediction
1. When preview is shown, press Escape
2. **Expected:** Preview disappears
3. **Expected:** No changes to buffer

### 4. Provider Switching

#### Test 4.1: Switch via Context Menu
1. Click Copilot icon in status bar
2. Note current provider mode
3. Click the switch option (e.g., "Use Copilot Next Edit Suggestions")
4. **Expected:** Provider switches successfully
5. Open menu again
6. **Expected:** Menu shows the opposite option now

#### Test 4.2: Switch via Settings
1. Open settings
2. Change `edit_prediction_provider` to `"zed"`
3. **Expected:** Zed AI icon appears (if signed in)
4. Change back to `"copilot_nes"`
5. **Expected:** Copilot icon appears

#### Test 4.3: Switch to None
1. Set `edit_prediction_provider` to `"none"`
2. **Expected:** No provider icon in status bar
3. **Expected:** No edit predictions appear

### 5. Multiple Suggestions (Cycling)

#### Test 5.1: Cycle Through Suggestions
1. Enable Copilot NES with eager mode
2. In a code file, position cursor where multiple edits might be suggested
3. Wait for preview to appear
4. Press Shift+Down Arrow
5. **Expected:** Next suggestion appears (if multiple available)
6. Press Shift+Up Arrow
7. **Expected:** Previous suggestion appears

#### Test 5.2: Accept Specific Suggestion
1. Cycle to a desired suggestion
2. Press Tab
3. **Expected:** That specific suggestion is accepted
4. **Expected:** Buffer is updated with the edit

### 6. UI Components

#### Test 6.1: Status Bar Icon
1. With Copilot NES active
2. Check status bar
3. **Expected:** Shows Copilot icon (same as traditional Copilot)
4. **Expected:** Icon indicates authentication status:
   - Green/Active: Authenticated and working
   - Gray/Disabled: Not authenticated or disabled
   - Red/Error: Error state

#### Test 6.2: Context Menu - Language Settings
1. Open Copilot context menu
2. **Expected:** Menu shows:
   - "Show Edit Predictions For" section
   - "This Buffer" toggle
   - Current language toggle (if applicable)
   - "All Files" toggle
3. Toggle "This Buffer"
4. **Expected:** Edit predictions enable/disable for current buffer

#### Test 6.3: Context Menu - Display Modes
1. Open Copilot context menu
2. **Expected:** Menu shows:
   - "Display Modes" section
   - "Eager" option with radio button
   - "Subtle" option with radio button
3. Click the non-active mode
4. **Expected:** Mode switches, appropriate icon shows selected state

### 7. Error Handling

#### Test 7.1: Not Signed In
1. Sign out of Copilot (via menu)
2. Try to enable Copilot NES
3. **Expected:** Appropriate error/prompt to sign in
4. **Expected:** No crashes or undefined behavior

#### Test 7.2: No Network
1. Disconnect from network
2. Try to get edit predictions
3. **Expected:** Graceful degradation (no predictions, or error message)
4. **Expected:** No crashes

#### Test 7.3: Invalid Settings
1. Try setting `edit_prediction_provider` to an invalid value
2. **Expected:** Settings validation error or fallback to default

### 8. Integration with Existing Features

#### Test 8.1: Undo/Redo
1. Accept an edit prediction
2. Press Cmd+Z (Undo)
3. **Expected:** Edit is undone
4. Press Cmd+Shift+Z (Redo)
5. **Expected:** Edit is reapplied

#### Test 8.2: With LSP Completions
1. Start typing to trigger LSP completions
2. **Expected:** In eager mode, predictions don't show when LSP completions are available
3. Dismiss LSP completions
4. **Expected:** Edit predictions appear

#### Test 8.3: Multi-Cursor Editing
1. Create multiple cursors (Cmd+Click)
2. **Expected:** Edit predictions work appropriately (may vary based on implementation)

### 9. Performance

#### Test 9.1: Throttling
1. Type rapidly in a file
2. **Expected:** LSP requests are throttled (not on every keystroke)
3. Check network activity or logs
4. **Expected:** Requests limited to ~300ms intervals

#### Test 9.2: Large Files
1. Open a large file (1000+ lines)
2. Enable edit predictions
3. **Expected:** No significant lag or performance degradation
4. Type and request predictions
5. **Expected:** Responsive editor

### 10. Language Support

#### Test 10.1: Disable for Language
1. Add to settings:
   ```json
   {
     "languages": {
       "Markdown": {
         "show_edit_predictions": false
       }
     }
   }
   ```
2. Open a Markdown file
3. **Expected:** No edit predictions appear
4. Open a JavaScript file
5. **Expected:** Edit predictions work normally

#### Test 10.2: Multiple Languages
1. Test in various file types:
   - JavaScript/TypeScript
   - Python
   - Rust
   - Go
   - etc.
2. **Expected:** Edit predictions work across supported languages

## Verification Checklist

After completing tests, verify:

- [ ] Settings correctly store and apply `copilot_nes` provider
- [ ] UI shows appropriate Copilot icon and status
- [ ] Context menu allows switching between Copilot modes
- [ ] Subtle mode only shows preview with modifier key
- [ ] Eager mode shows preview automatically
- [ ] Tab key accepts predictions
- [ ] Escape key discards predictions
- [ ] Shift+Up/Down cycles through suggestions
- [ ] Provider switching works via menu and settings
- [ ] Error states are handled gracefully
- [ ] No crashes or panics observed
- [ ] Performance is acceptable

## Known Limitations (Phase 3)

These are expected and will be addressed in later phases:

1. **Cross-file edits** - Not yet implemented (Phase 4+)
   - Currently only shows edits in current buffer
   
2. **Telemetry** - Not yet implemented (Phase 5)
   - Accept/reject events not sent to Copilot
   
3. **LSP server version** - May need specific version
   - Older copilot-language-server versions may not support NES

## Reporting Issues

When reporting issues, include:

1. **Zed version** - Help > About Zed
2. **Copilot status** - Icon state, sign-in status
3. **Settings** - Your edit prediction settings
4. **Steps to reproduce** - Clear, numbered steps
5. **Expected vs Actual** - What you expected vs what happened
6. **Logs** - Check Zed logs for errors (Help > View Logs)

## Debug Commands

Useful for troubleshooting:

```bash
# Run Zed with debug logging
RUST_LOG=copilot=debug,edit_prediction=debug /path/to/Zed

# Check LSP server version
# (TBD: specific command to check copilot-language-server version)

# View Zed logs
# Help > View Logs in Zed menu
```

## Success Criteria

Phase 3 is successful if:

✅ Users can select `copilot_nes` as their edit prediction provider
✅ Edit predictions appear and function correctly
✅ Subtle and Eager modes both work as expected
✅ UI properly reflects Copilot status and allows provider switching
✅ No regressions in existing Copilot or edit prediction functionality
✅ Performance is acceptable with no noticeable lag

## Next Steps

After Phase 3 testing is complete:
- Document any issues found
- Proceed to Phase 4: Advanced testing and polish
- Implement any critical fixes
- Consider cross-file edit support (future phase)