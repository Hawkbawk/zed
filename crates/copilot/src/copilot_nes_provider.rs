use crate::{Copilot, request::CopilotInlineEditItem};
use edit_prediction::{Direction, EditPrediction, EditPredictionProvider};
use gpui::{App, Context, Entity, EntityId, Task};
use language::{Anchor, Buffer, ToPoint};
use project::Project;
use std::{
    cmp,
    time::{Duration, Instant},
};

pub const COPILOT_NES_THROTTLE_TIMEOUT: Duration = Duration::from_millis(300);

pub struct CopilotNesProvider {
    copilot: Entity<Copilot>,
    #[allow(dead_code)] // Reserved for future cross-file support
    project: Entity<Project>,
    buffer_id: Option<EntityId>,
    edits: Vec<CopilotInlineEditItem>,
    active_edit_index: usize,
    next_pending_prediction_id: usize,
    pending_predictions: Vec<PendingPrediction>,
    last_request_timestamp: Instant,
}

struct PendingPrediction {
    id: usize,
    _task: Task<()>,
}

impl CopilotNesProvider {
    pub fn new(copilot: Entity<Copilot>, project: Entity<Project>) -> Self {
        Self {
            copilot,
            project,
            buffer_id: None,
            edits: Vec::new(),
            active_edit_index: 0,
            next_pending_prediction_id: 0,
            pending_predictions: Vec::new(),
            last_request_timestamp: Instant::now(),
        }
    }

    fn active_edit(&self) -> Option<&CopilotInlineEditItem> {
        self.edits.get(self.active_edit_index)
    }
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
        true
    }

    fn is_enabled(&self, _buffer: &Entity<Buffer>, _cursor_position: Anchor, cx: &App) -> bool {
        self.copilot.read(cx).status().is_authorized()
    }

    fn is_refreshing(&self) -> bool {
        !self.pending_predictions.is_empty()
    }

    fn refresh(
        &mut self,
        buffer: Entity<Buffer>,
        cursor_position: Anchor,
        _debounce: bool,
        cx: &mut Context<Self>,
    ) {
        let copilot = self.copilot.clone();
        let pending_prediction_id = self.next_pending_prediction_id;
        self.next_pending_prediction_id += 1;
        let last_request_timestamp = self.last_request_timestamp;

        let task = cx.spawn(async move |this, cx| {
            // Throttle requests
            if let Some(timeout) = (last_request_timestamp + COPILOT_NES_THROTTLE_TIMEOUT)
                .checked_duration_since(Instant::now())
            {
                cx.background_executor().timer(timeout).await;
            }

            this.update(cx, |this, _cx| {
                this.last_request_timestamp = Instant::now();
            })
            .ok();

            let result = copilot
                .update(cx, |copilot, cx| {
                    copilot.request_edit_suggestion(&buffer, cursor_position, cx)
                })
                .ok();

            if let Some(task) = result {
                match task.await {
                    Ok(edit_suggestion_result) => {
                        this.update(cx, |this, cx| {
                            if !edit_suggestion_result.edits.is_empty() {
                                this.buffer_id = Some(buffer.entity_id());
                                this.edits = edit_suggestion_result.edits;
                                this.active_edit_index = 0;
                                cx.notify();
                            }
                        })
                        .ok();
                    }
                    Err(error) => {
                        log::warn!("Error requesting next edit suggestion: {error}");
                    }
                }
            }

            this.update(cx, |this, cx| {
                // Remove completed prediction
                if let Some(pos) = this
                    .pending_predictions
                    .iter()
                    .position(|p| p.id == pending_prediction_id)
                {
                    this.pending_predictions.remove(pos);
                }
                cx.notify();
            })
            .ok();
        });

        // Maintain at most two pending predictions
        if self.pending_predictions.len() <= 1 {
            self.pending_predictions.push(PendingPrediction {
                id: pending_prediction_id,
                _task: task,
            });
        } else if self.pending_predictions.len() == 2 {
            self.pending_predictions.pop();
            self.pending_predictions.push(PendingPrediction {
                id: pending_prediction_id,
                _task: task,
            });
        }

        cx.notify();
    }

    fn cycle(
        &mut self,
        _buffer: Entity<Buffer>,
        _cursor_position: Anchor,
        direction: Direction,
        cx: &mut Context<Self>,
    ) {
        if self.edits.is_empty() {
            return;
        }

        match direction {
            Direction::Prev => {
                self.active_edit_index = if self.active_edit_index == 0 {
                    self.edits.len().saturating_sub(1)
                } else {
                    self.active_edit_index - 1
                };
            }
            Direction::Next => {
                self.active_edit_index = (self.active_edit_index + 1) % self.edits.len();
            }
        }

        cx.notify();
    }

    fn accept(&mut self, _cx: &mut Context<Self>) {
        // Clear current suggestions after acceptance
        // TODO: Add telemetry notification to Copilot (do we really want to though?)
        self.edits.clear();
        self.active_edit_index = 0;
        self.buffer_id = None;
        self.pending_predictions.clear();
    }

    fn discard(&mut self, _cx: &mut Context<Self>) {
        // Clear current suggestions
        // TODO: Add telemetry notification to Copilot (do we really want to though?)
        self.edits.clear();
        self.active_edit_index = 0;
        self.buffer_id = None;
        self.pending_predictions.clear();
    }

    fn suggest(
        &mut self,
        buffer: &Entity<Buffer>,
        cursor_position: Anchor,
        cx: &mut Context<Self>,
    ) -> Option<EditPrediction> {
        let buffer_id = buffer.entity_id();
        let buffer_snapshot = buffer.read(cx);

        // Check if we have valid edits for this buffer
        if Some(buffer_id) != self.buffer_id || self.edits.is_empty() {
            return None;
        }

        let active_edit = self.active_edit()?;

        // Get the buffer URI - for local files use abs_path, otherwise use buffer://
        let buffer_uri = if let Some(file) = buffer_snapshot.file().and_then(|f| f.as_local()) {
            lsp::Uri::from_file_path(file.abs_path(cx)).ok()?
        } else {
            format!("buffer://{}", buffer_id).parse().ok()?
        };

        // Check if the active edit is for the current buffer
        if active_edit.text_document.uri != buffer_uri {
            // This is a cross-file edit - we don't support jumping to other files yet
            // TODO: Implement proper cross-file jump support
            return None;
        }

        // Local edit in current buffer
        let cursor_row = cursor_position.to_point(&buffer_snapshot).row;

        // Find the edit closest to the cursor
        let (closest_edit_ix, closest_edit) =
            self.edits.iter().enumerate().min_by_key(|(_, edit)| {
                if edit.text_document.uri != buffer_uri {
                    return u32::MAX;
                }
                let start_point = language::point_from_lsp(edit.range.start).0;
                let end_point = language::point_from_lsp(edit.range.end).0;
                let distance_from_start = cursor_row.abs_diff(start_point.row);
                let distance_from_end = cursor_row.abs_diff(end_point.row);
                cmp::min(distance_from_start, distance_from_end)
            })?;

        // Group nearby edits (within 1 line)
        let closest_start = language::point_from_lsp(closest_edit.range.start).0;
        let closest_end = language::point_from_lsp(closest_edit.range.end).0;

        let mut edit_start_ix = closest_edit_ix;
        for edit in self.edits[..closest_edit_ix].iter().rev() {
            if edit.text_document.uri != buffer_uri {
                break;
            }
            let edit_end = language::point_from_lsp(edit.range.end).0;
            let distance = closest_start.row.saturating_sub(edit_end.row);
            if distance <= 1 {
                edit_start_ix -= 1;
            } else {
                break;
            }
        }

        let mut edit_end_ix = closest_edit_ix + 1;
        for edit in &self.edits[edit_end_ix..] {
            if edit.text_document.uri != buffer_uri {
                break;
            }
            let edit_start = language::point_from_lsp(edit.range.start).0;
            let distance = edit_start.row.saturating_sub(closest_end.row);
            if distance <= 1 {
                edit_end_ix += 1;
            } else {
                break;
            }
        }

        // Convert LSP ranges to Zed anchors
        let mut local_edits = Vec::new();
        for edit in &self.edits[edit_start_ix..edit_end_ix] {
            if edit.text_document.uri != buffer_uri {
                continue;
            }

            let start_point_utf16 = language::point_from_lsp(edit.range.start);
            let end_point_utf16 = language::point_from_lsp(edit.range.end);
            let start_point = buffer_snapshot.unclipped_point_utf16_to_point(start_point_utf16);
            let end_point = buffer_snapshot.unclipped_point_utf16_to_point(end_point_utf16);
            let start_offset = buffer_snapshot.point_to_offset(start_point);
            let end_offset = buffer_snapshot.point_to_offset(end_point);

            if start_offset > buffer_snapshot.len() || end_offset > buffer_snapshot.len() {
                continue;
            }

            let start_anchor = buffer_snapshot.anchor_before(start_offset);
            let end_anchor = buffer_snapshot.anchor_after(end_offset);
            local_edits.push((start_anchor..end_anchor, edit.text.clone()));
        }

        if local_edits.is_empty() {
            return None;
        }

        Some(EditPrediction::Local {
            id: None,
            edits: local_edits,
            edit_preview: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::CopilotInlineEditItem;
    use gpui::{AppContext as _, BackgroundExecutor, TestAppContext, UpdateGlobal};
    use language::Buffer;
    use project::Project;
    use settings::{AllLanguageSettingsContent, SettingsStore};

    #[gpui::test]
    async fn test_copilot_nes_cycle(_executor: BackgroundExecutor, cx: &mut TestAppContext) {
        init_test(cx, |_| {});

        let (copilot, _copilot_lsp) = Copilot::fake(cx);

        let fs = fs::FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let buffer = cx.new(|cx| Buffer::local("", cx));

        let copilot_provider =
            cx.new(|_| CopilotNesProvider::new(copilot.clone(), project.clone()));

        // Simulate having multiple edits in the provider
        copilot_provider.update(cx, |provider, _| {
            provider.edits = vec![
                CopilotInlineEditItem {
                    text_document: lsp::TextDocumentIdentifier {
                        uri: "file:///test.rs".parse().unwrap(),
                    },
                    range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 10)),
                    text: "edit1".to_string(),
                },
                CopilotInlineEditItem {
                    text_document: lsp::TextDocumentIdentifier {
                        uri: "file:///test.rs".parse().unwrap(),
                    },
                    range: lsp::Range::new(lsp::Position::new(5, 0), lsp::Position::new(5, 10)),
                    text: "edit2".to_string(),
                },
                CopilotInlineEditItem {
                    text_document: lsp::TextDocumentIdentifier {
                        uri: "file:///test.rs".parse().unwrap(),
                    },
                    range: lsp::Range::new(lsp::Position::new(10, 0), lsp::Position::new(10, 10)),
                    text: "edit3".to_string(),
                },
            ];
            provider.active_edit_index = 0;
        });

        let anchor = buffer.read_with(cx, |buffer, _| buffer.anchor_before(0));

        // Test cycling forward
        copilot_provider.read_with(cx, |provider, _| {
            assert_eq!(provider.active_edit_index, 0);
        });

        copilot_provider.update(cx, |provider, cx| {
            provider.cycle(buffer.clone(), anchor, Direction::Next, cx);
        });

        copilot_provider.read_with(cx, |provider, _| {
            assert_eq!(provider.active_edit_index, 1);
        });

        copilot_provider.update(cx, |provider, cx| {
            provider.cycle(buffer.clone(), anchor, Direction::Next, cx);
        });

        copilot_provider.read_with(cx, |provider, _| {
            assert_eq!(provider.active_edit_index, 2);
        });

        // Test wrap around
        copilot_provider.update(cx, |provider, cx| {
            provider.cycle(buffer.clone(), anchor, Direction::Next, cx);
        });

        copilot_provider.read_with(cx, |provider, _| {
            assert_eq!(provider.active_edit_index, 0);
        });

        // Test cycling backward
        copilot_provider.update(cx, |provider, cx| {
            provider.cycle(buffer.clone(), anchor, Direction::Prev, cx);
        });

        copilot_provider.read_with(cx, |provider, _| {
            assert_eq!(provider.active_edit_index, 2);
        });
    }

    #[gpui::test]
    async fn test_copilot_nes_accept_discard(
        _executor: BackgroundExecutor,
        cx: &mut TestAppContext,
    ) {
        init_test(cx, |_| {});

        let (copilot, _copilot_lsp) = Copilot::fake(cx);

        let fs = fs::FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let buffer = cx.new(|cx| Buffer::local("", cx));

        let copilot_provider =
            cx.new(|_| CopilotNesProvider::new(copilot.clone(), project.clone()));

        // Set up some edits
        let buffer_id = buffer.entity_id();
        copilot_provider.update(cx, |provider, _| {
            provider.edits = vec![CopilotInlineEditItem {
                text_document: lsp::TextDocumentIdentifier {
                    uri: "file:///test.rs".parse().unwrap(),
                },
                range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 10)),
                text: "edit1".to_string(),
            }];
            provider.buffer_id = Some(buffer_id);
        });

        // Test accept clears state
        copilot_provider.update(cx, |provider, cx| {
            provider.accept(cx);
        });

        copilot_provider.read_with(cx, |provider, _| {
            assert!(provider.edits.is_empty());
            assert_eq!(provider.active_edit_index, 0);
            assert_eq!(provider.buffer_id, None);
        });

        // Set up edits again
        copilot_provider.update(cx, |provider, _| {
            provider.edits = vec![CopilotInlineEditItem {
                text_document: lsp::TextDocumentIdentifier {
                    uri: "file:///test.rs".parse().unwrap(),
                },
                range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 10)),
                text: "edit1".to_string(),
            }];
            provider.buffer_id = Some(buffer_id);
        });

        // Test discard clears state
        copilot_provider.update(cx, |provider, cx| {
            provider.discard(cx);
        });

        copilot_provider.read_with(cx, |provider, _| {
            assert!(provider.edits.is_empty());
            assert_eq!(provider.active_edit_index, 0);
            assert_eq!(provider.buffer_id, None);
        });
    }

    fn init_test(cx: &mut TestAppContext, f: fn(&mut AllLanguageSettingsContent)) {
        cx.update(|cx| {
            let store = SettingsStore::test(cx);
            cx.set_global(store);
            theme::init(theme::LoadThemes::JustBase, cx);
            client::init_settings(cx);
            language::init(cx);
            editor::init_settings(cx);
            Project::init_settings(cx);
            workspace::init_settings(cx);
            SettingsStore::update_global(cx, |store: &mut SettingsStore, cx| {
                store.update_user_settings(cx, |settings| f(&mut settings.project.all_languages));
            });
        });
    }
}
