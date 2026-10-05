//! A multi-line, word-wrapping text field for the vault's Markdown notes.
//!
//! Same model as [`crate::input::TextInput`] (byte ranges, UTF-16 for the platform IME) and
//! the same editing actions; Enter inserts a line break and Up/Down move between visual rows.
//! It grows with its text: the panel around it scrolls.

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, KeyBinding,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    SharedString, Style, TextRun, UTF16Selection, Window, WrappedLine, actions, div, fill, point,
    prelude::*, px, relative, size,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::input::{
    Backspace, Copy, Cut, Delete, End, Home, InputEvent, Left, Paste, Right, SelectAll, SelectLeft,
    SelectRight,
};
use crate::theme::{Theme, hsla};

actions!(text_area, [Up, Down, SelectUp, SelectDown, Newline]);

const CONTEXT: &str = "TextArea";
const MIN_ROWS: usize = 8;

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some(CONTEXT)),
        KeyBinding::new("delete", Delete, Some(CONTEXT)),
        KeyBinding::new("left", Left, Some(CONTEXT)),
        KeyBinding::new("right", Right, Some(CONTEXT)),
        KeyBinding::new("up", Up, Some(CONTEXT)),
        KeyBinding::new("down", Down, Some(CONTEXT)),
        KeyBinding::new("shift-left", SelectLeft, Some(CONTEXT)),
        KeyBinding::new("shift-right", SelectRight, Some(CONTEXT)),
        KeyBinding::new("shift-up", SelectUp, Some(CONTEXT)),
        KeyBinding::new("shift-down", SelectDown, Some(CONTEXT)),
        KeyBinding::new("secondary-a", SelectAll, Some(CONTEXT)),
        KeyBinding::new("secondary-v", Paste, Some(CONTEXT)),
        KeyBinding::new("secondary-c", Copy, Some(CONTEXT)),
        KeyBinding::new("secondary-x", Cut, Some(CONTEXT)),
        KeyBinding::new("home", Home, Some(CONTEXT)),
        KeyBinding::new("end", End, Some(CONTEXT)),
        KeyBinding::new("enter", Newline, Some(CONTEXT)),
    ]);
}

/// The last painted layout: one wrapped line per hard line.
#[derive(Default)]
struct Layout {
    lines: Vec<WrappedLine>,
    /// Byte offset where each hard line starts.
    starts: Vec<usize>,
    /// Top of each hard line, relative to the text origin.
    tops: Vec<Pixels>,
    line_height: Pixels,
    origin: Point<Pixels>,
}

impl Layout {
    /// Hard line holding byte `offset`.
    fn line_of(&self, offset: usize) -> usize {
        self.starts
            .iter()
            .rposition(|&s| s <= offset)
            .unwrap_or_default()
    }

    /// Position of byte `offset`, relative to the text origin (top of its row).
    fn position(&self, offset: usize) -> Option<Point<Pixels>> {
        let i = self.line_of(offset);
        let p = self
            .lines
            .get(i)?
            .position_for_index(offset.saturating_sub(self.starts[i]), self.line_height)?;
        Some(point(p.x, p.y + self.tops[i]))
    }

    /// Closest byte offset to `p` (relative to the text origin).
    fn index_at(&self, p: Point<Pixels>, len: usize) -> usize {
        if p.y < px(0.) {
            return 0;
        }
        for (i, line) in self.lines.iter().enumerate() {
            let rows = line.wrap_boundaries().len() + 1;
            #[allow(clippy::cast_precision_loss)] // a few rows
            let bottom = self.tops[i] + self.line_height * rows as f32;
            if p.y < bottom {
                let local = point(p.x.max(px(0.)), p.y - self.tops[i]);
                let ix = match line.closest_index_for_position(local, self.line_height) {
                    Ok(ix) | Err(ix) => ix,
                };
                return self.starts[i] + ix.min(line.len());
            }
        }
        len
    }
}

pub struct TextArea {
    focus_handle: FocusHandle,
    content: String,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    layout: Layout,
    /// Width the text was last wrapped to.
    wrap_width: Pixels,
    is_selecting: bool,
    pub theme: &'static Theme,
}

impl EventEmitter<InputEvent> for TextArea {}

impl TextArea {
    pub fn new(
        placeholder: impl Into<SharedString>,
        theme: &'static Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: String::new(),
            placeholder: placeholder.into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            layout: Layout::default(),
            wrap_width: px(220.),
            is_selecting: false,
            theme,
        }
    }

    /// Replaces the contents without emitting [`InputEvent::Changed`].
    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.content == text {
            return;
        }
        text.clone_into(&mut self.content);
        self.selected_range = 0..0;
        self.selection_reversed = false;
        self.marked_range = None;
        cx.notify();
    }

    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus_handle.is_focused(window)
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }

    /// Offset one visual row above (`dir < 0`) or below the cursor, at the same x.
    fn vertical(&self, dir: f32) -> usize {
        let cursor = self.cursor_offset();
        let Some(p) = self.layout.position(cursor) else {
            return cursor;
        };
        let h = self.layout.line_height;
        let target = point(p.x, p.y + h / 2. + h * dir);
        if target.y < px(0.) {
            return 0;
        }
        self.layout.index_at(target, self.content.len())
    }

    fn line_start(&self, offset: usize) -> usize {
        self.content[..offset].rfind('\n').map_or(0, |i| i + 1)
    }

    fn line_end(&self, offset: usize) -> usize {
        self.content[offset..]
            .find('\n')
            .map_or(self.content.len(), |i| offset + i)
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.vertical(-1.), cx);
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.vertical(1.), cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.vertical(-1.), cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.vertical(1.), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selected_range = 0..self.content.len();
        self.selection_reversed = false;
        cx.notify();
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.line_start(self.cursor_offset()), cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.line_end(self.cursor_offset()), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let prev = self.previous_boundary(self.cursor_offset());
            if self.cursor_offset() == prev {
                return;
            }
            self.select_to(prev, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            if self.cursor_offset() == next {
                return;
            }
            self.select_to(next, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_text_in_range(None, "\n", window, cx);
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text.replace("\r\n", "\n"), window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_owned(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            self.copy(&Copy, window, cx);
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn index_for_mouse(&self, p: Point<Pixels>) -> usize {
        self.layout
            .index_at(p - self.layout.origin, self.content.len())
    }

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
        self.is_selecting = true;
        let ix = self.index_for_mouse(ev.position);
        if ev.modifiers.shift {
            self.select_to(ix, cx);
        } else {
            self.move_to(ix, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse(ev.position), cx);
        }
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8 = 0;
        let mut utf16 = 0;
        for ch in self.content.chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += ch.len_utf16();
            utf8 += ch.len_utf8();
        }
        utf8
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0;
        let mut utf8 = 0;
        for ch in self.content.chars() {
            if utf8 >= offset {
                break;
            }
            utf8 += ch.len_utf8();
            utf16 += ch.len_utf16();
        }
        utf16
    }

    fn range_to_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(r.start)..self.offset_to_utf16(r.end)
    }

    fn range_from_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(r.start)..self.offset_from_utf16(r.end)
    }
}

impl EntityInputHandler for TextArea {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        self.content.replace_range(range.clone(), new_text);
        let end = range.start + new_text.len();
        self.selected_range = end..end;
        self.selection_reversed = false;
        self.marked_range = None;
        cx.emit(InputEvent::Changed(self.content.clone()));
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        self.content.replace_range(range.clone(), new_text);
        self.marked_range =
            (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .map_or_else(
                || range.start + new_text.len()..range.start + new_text.len(),
                |r| r.start + range.start..r.end + range.start,
            );
        cx.emit(InputEvent::Changed(self.content.clone()));
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let a = self.layout.position(range.start)?;
        let b = self.layout.position(range.end)?;
        let o = self.layout.origin;
        Some(Bounds::from_corners(
            o + a,
            o + point(b.x, b.y + self.layout.line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.offset_to_utf16(self.index_for_mouse(p)))
    }
}

struct TextAreaElement {
    area: Entity<TextArea>,
}

struct Shaped {
    lines: Vec<WrappedLine>,
    starts: Vec<usize>,
    placeholder: bool,
}

struct Prepainted {
    quads: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
}

impl IntoElement for TextAreaElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextAreaElement {
    type RequestLayoutState = Shaped;
    type PrepaintState = Prepainted;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let area = self.area.read(cx);
        let style = window.text_style();
        let placeholder = area.content.is_empty();
        let (text, color) = if placeholder {
            (area.placeholder.to_string(), hsla(area.theme.muted))
        } else {
            (area.content.clone(), style.color)
        };
        let starts: Vec<usize> = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        let run = TextRun {
            len: text.len(),
            font: style.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        let lines: Vec<WrappedLine> = window
            .text_system()
            .shape_text(text.into(), font_size, &[run], Some(area.wrap_width), None)
            .map(|v| v.into_iter().collect())
            .unwrap_or_default();
        let rows: usize = lines
            .iter()
            .map(|l| l.wrap_boundaries().len() + 1)
            .sum::<usize>()
            .max(MIN_ROWS);
        let mut s = Style::default();
        s.size.width = relative(1.).into();
        #[allow(clippy::cast_precision_loss)] // a few rows
        let height = window.line_height() * rows as f32;
        s.size.height = height.into();
        (
            window.request_layout(s, [], cx),
            Shaped {
                lines,
                starts,
                placeholder,
            },
        )
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        shaped: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let line_height = window.line_height();
        let width_changed = self.area.update(cx, |a, _| {
            let changed = (a.wrap_width - bounds.size.width).abs() > px(0.5);
            a.wrap_width = bounds.size.width;
            changed
        });
        if width_changed {
            window.refresh();
        }
        let mut tops = Vec::with_capacity(shaped.lines.len());
        let mut y = px(0.);
        for l in &shaped.lines {
            tops.push(y);
            #[allow(clippy::cast_precision_loss)]
            {
                y += line_height * (l.wrap_boundaries().len() + 1) as f32;
            }
        }
        let layout = Layout {
            lines: Vec::new(),
            starts: shaped.starts.clone(),
            tops,
            line_height,
            origin: bounds.origin,
        };
        let area = self.area.read(cx);
        let accent = hsla(area.theme.accent);
        let (range, cursor) = (area.selected_range.clone(), area.cursor_offset());
        // Positions need the shaped lines: borrow them for the duration of the lookups.
        let tmp = Layout {
            lines: std::mem::take(&mut shaped.lines),
            ..layout
        };
        let mut quads = Vec::new();
        let mut caret = None;
        if !shaped.placeholder {
            if range.is_empty() {
                if let Some(p) = tmp.position(cursor) {
                    caret = Some(fill(
                        Bounds::new(bounds.origin + p, size(px(1.5), line_height)),
                        accent,
                    ));
                }
            } else if let (Some(a), Some(b)) = (tmp.position(range.start), tmp.position(range.end))
            {
                let right = bounds.size.width;
                let mut row = a.y;
                while row <= b.y {
                    let x0 = if row == a.y { a.x } else { px(0.) };
                    let x1 = if row == b.y { b.x } else { right };
                    quads.push(fill(
                        Bounds::from_corners(
                            bounds.origin + point(x0, row),
                            bounds.origin + point(x1.max(x0 + px(3.)), row + line_height),
                        ),
                        accent.opacity(0.25),
                    ));
                    row += line_height;
                }
            }
        } else if area.is_focused(window) {
            caret = Some(fill(
                Bounds::new(bounds.origin, size(px(1.5), line_height)),
                accent,
            ));
        }
        shaped.lines = tmp.lines;
        self.area.update(cx, |a, _| {
            a.layout = Layout {
                lines: Vec::new(),
                starts: tmp.starts,
                tops: tmp.tops,
                line_height,
                origin: bounds.origin,
            };
        });
        Prepainted {
            quads,
            cursor: caret,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        shaped: &mut Self::RequestLayoutState,
        pre: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.area.read(cx).focus_handle.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.area.clone()),
            cx,
        );
        for q in pre.quads.drain(..) {
            window.paint_quad(q);
        }
        let line_height = window.line_height();
        let tops = self.area.read(cx).layout.tops.clone();
        for (line, top) in shaped.lines.iter().zip(tops) {
            line.paint(
                bounds.origin + point(px(0.), top),
                line_height,
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            )
            .ok();
        }
        if focus.is_focused(window)
            && let Some(c) = pre.cursor.take()
        {
            window.paint_quad(c);
        }
        let lines = std::mem::take(&mut shaped.lines);
        let placeholder = shaped.placeholder;
        self.area.update(cx, |a, _| {
            // Hit-testing an empty note: there is nothing to measure.
            if !placeholder {
                a.layout.lines = lines;
            }
        });
    }
}

impl Render for TextArea {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let focused = self.focus_handle.is_focused(window);
        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .p(px(8.))
            .rounded(px(6.))
            .border_1()
            .border_color(hsla(if focused { t.accent } else { t.line }))
            .bg(hsla(t.paper))
            .text_color(hsla(t.ink))
            .text_size(px(12.5))
            .line_height(px(18.))
            .child(TextAreaElement { area: cx.entity() })
    }
}

impl Focusable for TextArea {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
