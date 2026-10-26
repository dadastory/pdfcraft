//! Asynchronous host font selection. Downloads outlive a view; applying the selection does not.

use std::sync::mpsc::{Receiver, Sender, TryRecvError};

use pdfcraft_engine::{AddedText, Edit, FontFamily};

use crate::{PdfCraftApp, QuickTool, edit_text_ui::LineEditor, files::PickTarget};

pub type HostFontFn = Box<dyn Fn(usize, Sender<Result<(), String>>)>;

pub(crate) enum FontSelection {
    Paragraph { before: Box<LineEditor>, resource: String },
    Added { before: AddedText, after: Box<AddedText>, selected: Option<(usize, usize)> },
}

pub(crate) struct PendingFont {
    target: Option<PickTarget>,
    selection: FontSelection,
    result: Receiver<Result<(), String>>,
}

impl PdfCraftApp {
    /// Called on every new font choice, including choices that are already loaded.
    pub(crate) fn choose_packaged_font(&mut self, font: usize, selection: FontSelection) {
        self.pending_font = None;
        let Some(face) = pdfcraft_fonts::CRAFT_FONTS.get(font) else { return };
        if face.is_loaded() {
            self.apply_font_selection(selection);
            return;
        }
        let Some(host) = &self.host_font_request else {
            self.notify_error("The selected font is not loaded. Retry after connecting the font host.");
            return;
        };
        let (reply, result) = std::sync::mpsc::channel();
        host(font, reply);
        self.pending_font = Some(PendingFont { target: self.pick_target(self.active_ids().map(|(_, id)| id)), selection, result });
    }

    pub(crate) fn process_font_requests(&mut self) {
        let Some(pending) = self.pending_font.take() else { return };
        if self.pick_target(self.active_ids().map(|(_, id)| id)) != pending.target {
            return;
        }
        match pending.result.try_recv() {
            Ok(result) => {
                if !self.still_pick_target(pending.target) {
                    return;
                }
                match result {
                    Ok(()) => self.apply_font_selection(pending.selection),
                    Err(error) => self.notify_error(error),
                }
            }
            Err(TryRecvError::Empty) => self.pending_font = Some(pending),
            Err(TryRecvError::Disconnected) => self.notify_error("Font loading stopped. Select the font again to retry."),
        }
    }

    fn apply_font_selection(&mut self, selection: FontSelection) {
        let Some((view, _)) = self.active_ids() else { return };
        match selection {
            FontSelection::Paragraph { before, resource } => {
                let Some(current) = self.views.get(view).and_then(|v| v.line_editor.as_ref()) else { return };
                // Layout, scroll and caret may change while loading; text and formatting may not.
                if current.page != before.page || current.block != before.block || current.text != before.text || current.style() != before.style() {
                    return;
                }
                let mut editor = current.clone();
                editor.font_resource = Some(resource);
                editor.look.bold = false;
                editor.look.italic = false;
                let ctx = self.ctx.clone();
                self.apply_line_format(view, editor, ctx.as_ref());
            }
            FontSelection::Added { before, after, selected } => {
                let after = *after;
                if self.views.get(view).is_none_or(|v| v.content.selected != selected || v.line_editor.is_some()) {
                    return;
                }
                if let Some((page, index)) = selected {
                    if self.apply_edit(Edit::UpdateContent { page, index, content: pdfcraft_engine::AddedContent::Text(after.clone()) }) {
                        self.text_style = AddedText { text: String::new(), rect: [0.0; 4], ..after };
                    }
                } else if self.quick_tool == QuickTool::AddText && self.text_style == before {
                    self.text_style = after;
                }
            }
        }
    }

    pub(crate) fn defer_added_font(&mut self, before: &AddedText, after: &AddedText, selected: Option<(usize, usize)>) -> bool {
        let FontFamily::Packaged(index) = after.family else { return false };
        if before.family == after.family || pdfcraft_fonts::CRAFT_FONTS.get(index).is_none_or(|face| face.is_loaded()) {
            return false;
        }
        self.choose_packaged_font(index, FontSelection::Added { before: before.clone(), after: Box::new(after.clone()), selected });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> PdfCraftApp {
        let mut app = PdfCraftApp::new();
        let bytes = app.session.create_blank(300.0, 240.0, 1).unwrap();
        app.open_bytes("font-test.pdf", None, bytes.as_ref().clone()).unwrap();
        app.quick_tool = QuickTool::AddText;
        app
    }

    fn pending(app: &mut PdfCraftApp, family: FontFamily) -> Sender<Result<(), String>> {
        let before = app.text_style.clone();
        let after = AddedText { family, ..before.clone() };
        let (reply, result) = std::sync::mpsc::channel();
        app.pending_font = Some(PendingFont {
            target: app.pick_target(app.active_ids().map(|(_, id)| id)),
            selection: FontSelection::Added { before, after: Box::new(after), selected: None },
            result,
        });
        reply
    }

    #[test]
    fn a_late_font_never_changes_a_newer_document_revision_or_closed_view() {
        let mut app = app();
        let original = app.text_style.clone();
        let reply = pending(&mut app, FontFamily::Times);
        let (_, id) = app.active_ids().unwrap();
        app.session.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
        reply.send(Ok(())).unwrap();
        app.process_font_requests();
        assert_eq!(app.text_style, original);
        let reply = pending(&mut app, FontFamily::Times);
        app.active = None;
        reply.send(Ok(())).unwrap();
        app.process_font_requests();
        assert_eq!(app.text_style, original);
    }

    #[test]
    fn a_new_font_choice_supersedes_the_old_request_without_cancelling_its_download() {
        let mut app = app();
        let old = pending(&mut app, FontFamily::Times);
        let current = pending(&mut app, FontFamily::Courier);
        assert!(old.send(Ok(())).is_err(), "only the stale delivery is dropped");
        app.process_font_requests();
        assert_eq!(app.text_style.family, FontFamily::Helvetica);
        current.send(Ok(())).unwrap();
        app.process_font_requests();
        assert_eq!(app.text_style.family, FontFamily::Courier);
    }

    #[test]
    fn failed_loads_can_be_retried_and_a_changed_draft_is_not_overwritten() {
        let mut app = app();
        pending(&mut app, FontFamily::Times).send(Err("network unavailable".into())).unwrap();
        app.process_font_requests();
        assert_eq!(app.text_style.family, FontFamily::Helvetica);
        pending(&mut app, FontFamily::Times).send(Ok(())).unwrap();
        app.process_font_requests();
        assert_eq!(app.text_style.family, FontFamily::Times);
        let reply = pending(&mut app, FontFamily::Courier);
        app.text_style.size = 24.0;
        reply.send(Ok(())).unwrap();
        app.process_font_requests();
        assert_eq!(app.text_style.family, FontFamily::Times);
        assert_eq!(app.text_style.size, 24.0);
    }
}
