//! The editor window: custom title bar (the window has no system decorations), palette,
//! canvas, properties panel and status bar. The network vault parts live in [`hub`].

mod hub;

use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui::{
    AnyElement, App, ClickEvent, Context, CursorStyle, Decorations, Entity, FocusHandle, Focusable,
    HitboxBehavior, Hsla, KeyBinding, KeyDownEvent, KeyUpEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PathPromptOptions, Pixels, ResizeEdge, ScrollDelta,
    ScrollWheelEvent, SharedString, Size, Subscription, Window, actions, canvas, div, point,
    prelude::*, px, svg,
};
use infraplot_model::catalog::NodeGroup;
use infraplot_model::ops::ElementType;
use infraplot_model::{
    Arrow, Diagram, Look, NodeKind, Route, StrokeStyle, TextSize, ZoneKind, catalog::parse_hex,
};

use crate::app::hub::{Focus, Hub};
use crate::input::{InputEvent, TextInput};
use crate::paint::{self, Scene};
use crate::settings::Settings;
use crate::state::{Button, Editor, Mods, Tool};
use crate::theme::{Theme, ThemeName, hsla, mix};

actions!(
    infra_plot,
    [
        NewFile,
        OpenFile,
        Save,
        SaveAs,
        Undo,
        Redo,
        Duplicate,
        DeleteSelection,
        Cancel,
        FitView,
        ZoomIn,
        ZoomOut,
        ToolSelect,
        ToolHand,
        ToolZone,
        ToolConnect,
        ToolLine,
        ToolNote,
        FinishLine,
        CycleTheme,
        Quit,
    ]
);

const CANVAS: &str = "Canvas";
const EDITOR: &str = "Editor";

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-n", NewFile, Some(EDITOR)),
        KeyBinding::new("secondary-o", OpenFile, Some(EDITOR)),
        KeyBinding::new("secondary-s", Save, Some(EDITOR)),
        KeyBinding::new("secondary-shift-s", SaveAs, Some(EDITOR)),
        KeyBinding::new("secondary-z", Undo, Some(EDITOR)),
        KeyBinding::new("secondary-shift-z", Redo, Some(EDITOR)),
        KeyBinding::new("secondary-y", Redo, Some(EDITOR)),
        KeyBinding::new("secondary-q", Quit, Some(EDITOR)),
        KeyBinding::new("secondary-=", ZoomIn, Some(EDITOR)),
        KeyBinding::new("secondary-+", ZoomIn, Some(EDITOR)),
        KeyBinding::new("secondary--", ZoomOut, Some(EDITOR)),
        KeyBinding::new("secondary-0", FitView, Some(EDITOR)),
        KeyBinding::new("escape", Cancel, Some(EDITOR)),
        // Single keys only on the canvas, so they still type in text fields.
        KeyBinding::new("secondary-d", Duplicate, Some(CANVAS)),
        KeyBinding::new("v", ToolSelect, Some(CANVAS)),
        KeyBinding::new("h", ToolHand, Some(CANVAS)),
        KeyBinding::new("r", ToolZone, Some(CANVAS)),
        KeyBinding::new("c", ToolConnect, Some(CANVAS)),
        KeyBinding::new("l", ToolLine, Some(CANVAS)),
        KeyBinding::new("t", ToolNote, Some(CANVAS)),
        KeyBinding::new("f", FitView, Some(CANVAS)),
        KeyBinding::new("delete", DeleteSelection, Some(CANVAS)),
        KeyBinding::new("backspace", DeleteSelection, Some(CANVAS)),
        KeyBinding::new("enter", FinishLine, Some(CANVAS)),
    ]);
}

/// Example diagrams from `examples/`, compiled in.
pub const EXAMPLES: &[(&str, &str)] = &[
    ("hello.json", include_str!("../../../examples/hello.json")),
    (
        "three-tier.toml",
        include_str!("../../../examples/three-tier.toml"),
    ),
    (
        "k8s-platform.toml",
        include_str!("../../../examples/k8s-platform.toml"),
    ),
    (
        "event-driven.toml",
        include_str!("../../../examples/event-driven.toml"),
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Title,
    Description,
    Id,
    Label,
    Color,
}

/// Something that would discard unsaved changes, waiting for confirmation.
#[derive(Debug, Clone)]
enum Pending {
    New,
    Open,
    Example(usize),
    NewVault,
    OpenVault,
    /// A vault diagram, optionally selecting the node of a host.
    OpenDiagram(PathBuf, Option<String>),
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Menu {
    Examples,
    NodeKind,
    Vault,
    Web,
}

struct Toast {
    text: String,
    error: bool,
    seq: u64,
}

struct Inputs {
    title: Entity<TextInput>,
    description: Entity<TextInput>,
    id: Entity<TextInput>,
    label: Entity<TextInput>,
    color: Entity<TextInput>,
}

impl Inputs {
    fn all(&self) -> [(Field, &Entity<TextInput>); 5] {
        [
            (Field::Title, &self.title),
            (Field::Description, &self.description),
            (Field::Id, &self.id),
            (Field::Label, &self.label),
            (Field::Color, &self.color),
        ]
    }
}

pub struct InfraPlot {
    editor: Editor,
    theme: ThemeName,
    canvas_focus: FocusHandle,
    canvas_origin: gpui::Point<Pixels>,
    needs_fit: bool,
    inputs: Inputs,
    /// The (selection, field) currently being typed into: the first keystroke takes an undo
    /// checkpoint, the rest of the word goes into the same step.
    editing: Option<(Option<String>, Field)>,
    toast: Option<Toast>,
    toast_seq: u64,
    menu: Option<Menu>,
    confirm: Option<Pending>,
    window_title: String,
    /// Left button pressed on the title bar: the next move drags the window.
    title_drag: bool,
    settings: Settings,
    hub: Hub,
    _subscriptions: Vec<Subscription>,
}

/// Note text is edited on one line; `\n` stands for a line break.
fn note_to_field(text: &str) -> String {
    text.replace('\n', "\\n")
}

fn field_to_note(text: &str) -> String {
    text.replace("\\n", "\n")
}

impl InfraPlot {
    pub fn new(
        doc: Diagram,
        path: Option<PathBuf>,
        theme: ThemeName,
        settings: Settings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let t = theme.theme();
        let mut subs = Vec::new();
        let mut input = |placeholder: &str, field: Field, cx: &mut Context<Self>| {
            let e = cx.new(|cx| TextInput::new(placeholder.to_owned(), t, cx));
            subs.push(cx.subscribe(&e, move |this, _, ev: &InputEvent, cx| {
                this.on_input(field, ev, cx);
            }));
            e
        };
        let inputs = Inputs {
            title: input("Untitled diagram", Field::Title, cx),
            description: input("Description", Field::Description, cx),
            id: input("id", Field::Id, cx),
            label: input("Label", Field::Label, cx),
            color: input("#rrggbb (default)", Field::Color, cx),
        };
        let web_url = settings
            .web_url
            .clone()
            .unwrap_or_else(|| crate::web::DEFAULT_URL.to_owned());
        let hub = Hub::new(t, &web_url, cx, &mut subs);
        let this = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            this.update(cx, |app, cx| {
                if app.editor.dirty {
                    app.confirm = Some(Pending::Quit);
                    cx.notify();
                    false
                } else {
                    true
                }
            })
            .unwrap_or(true)
        });
        let canvas_focus = cx.focus_handle();
        window.focus(&canvas_focus, cx);
        Self {
            editor: Editor::new(doc, path),
            theme,
            canvas_focus,
            canvas_origin: point(px(0.), px(0.)),
            needs_fit: true,
            inputs,
            editing: None,
            toast: None,
            toast_seq: 0,
            menu: None,
            confirm: None,
            window_title: String::new(),
            title_drag: false,
            settings,
            hub,
            _subscriptions: subs,
        }
    }

    fn t(&self) -> &'static Theme {
        self.theme.theme()
    }

    // ------------------------------------------------------------ feedback

    fn notify_msg(&mut self, text: impl Into<String>, error: bool, cx: &mut Context<Self>) {
        self.toast_seq += 1;
        let seq = self.toast_seq;
        self.toast = Some(Toast {
            text: text.into(),
            error,
            seq,
        });
        cx.notify();
        let ttl = Duration::from_secs(if error { 8 } else { 3 });
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(ttl).await;
            this.update(cx, |app, cx| {
                if app.toast.as_ref().is_some_and(|t| t.seq == seq) {
                    app.toast = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    // ------------------------------------------------------------ files

    fn load_doc(&mut self, doc: Diagram, path: Option<PathBuf>, cx: &mut Context<Self>) {
        self.editor.load(doc, path);
        self.needs_fit = true;
        self.editing = None;
        self.menu = None;
        cx.notify();
    }

    pub fn open_path(&mut self, path: &Path, cx: &mut Context<Self>) {
        match Diagram::load(path) {
            Ok(doc) => {
                self.load_doc(doc, Some(path.to_path_buf()), cx);
                self.notify_msg(format!("Opened {}", path.display()), false, cx);
            }
            Err(e) => self.notify_msg(format!("{}: {e}", path.display()), true, cx),
        }
    }

    /// Runs `pending` now, or asks first if there are unsaved changes.
    fn guard(&mut self, pending: Pending, cx: &mut Context<Self>) {
        self.menu = None;
        if self.editor.dirty {
            self.confirm = Some(pending);
            cx.notify();
        } else {
            self.run_pending(pending, cx);
        }
    }

    fn run_pending(&mut self, pending: Pending, cx: &mut Context<Self>) {
        self.confirm = None;
        match pending {
            Pending::New => self.load_doc(Diagram::new("Untitled diagram"), None, cx),
            Pending::Open => Self::open_dialog(cx),
            Pending::Example(i) => {
                let Some(&(name, src)) = EXAMPLES.get(i) else {
                    return;
                };
                let format = infraplot_model::Format::detect(Path::new(name), src);
                match Diagram::parse(src, format) {
                    Ok(doc) => self.load_doc(doc, None, cx),
                    Err(e) => self.notify_msg(format!("{name}: {e}"), true, cx),
                }
            }
            Pending::NewVault => Self::vault_dialog(true, cx),
            Pending::OpenVault => Self::vault_dialog(false, cx),
            Pending::OpenDiagram(path, host) => self.open_vault_diagram(&path, host.as_deref(), cx),
            Pending::Quit => cx.quit(),
        }
        cx.notify();
    }

    fn open_dialog(cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open diagram".into()),
        });
        cx.spawn(async move |this, cx| {
            let res = rx.await;
            this.update(cx, |app, cx| match res {
                Ok(Ok(Some(paths))) => {
                    if let Some(p) = paths.first() {
                        app.open_path(p, cx);
                    }
                }
                Ok(Err(e)) => app.notify_msg(format!("No file dialog available: {e}"), true, cx),
                _ => {}
            })
            .ok();
        })
        .detach();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        match self.editor.path.clone() {
            Some(p) => self.save_to(&p, cx),
            None => self.save_as(cx),
        }
    }

    fn save_to(&mut self, path: &Path, cx: &mut Context<Self>) {
        match self.editor.doc.save(path) {
            Ok(()) => {
                self.editor.path = Some(path.to_path_buf());
                self.editor.dirty = false;
                self.notify_msg(format!("Saved {}", path.display()), false, cx);
            }
            Err(e) => self.notify_msg(format!("Could not save {}: {e}", path.display()), true, cx),
        }
    }

    fn save_as(&mut self, cx: &mut Context<Self>) {
        let dir = self
            .editor
            .path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        let name = format!("{}.toml", slug(&self.editor.doc.title));
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn(async move |this, cx| {
            let res = rx.await;
            this.update(cx, |app, cx| match res {
                Ok(Ok(Some(mut p))) => {
                    if p.extension().is_none() {
                        p.set_extension("toml");
                    }
                    app.save_to(&p, cx);
                }
                Ok(Err(e)) => app.notify_msg(format!("No file dialog available: {e}"), true, cx),
                _ => {}
            })
            .ok();
        })
        .detach();
    }

    // ------------------------------------------------------------ properties

    fn on_input(&mut self, field: Field, ev: &InputEvent, cx: &mut Context<Self>) {
        let sel = self.editor.selection.clone();
        match ev {
            InputEvent::Submitted => {
                if field == Field::Id
                    && let Some(old) = sel
                {
                    let new = self.inputs.id.read(cx).text().trim().to_owned();
                    let mut renamed = self.editor.doc.clone();
                    if new == old {
                    } else if renamed.rename_id(&old, &new) {
                        self.editor.edit(|d| *d = renamed);
                        self.editor.selection = Some(new);
                    } else {
                        self.notify_msg(format!("`{new}` is taken or not a valid id"), true, cx);
                    }
                }
                self.editing = None;
                cx.notify();
            }
            InputEvent::Changed(text) => {
                if field == Field::Id {
                    return;
                }
                if field == Field::Color && !text.is_empty() && parse_hex(text).is_none() {
                    return; // wait until it is a whole colour
                }
                let key = (sel.clone(), field);
                if self.editing.as_ref() != Some(&key) {
                    self.editor.checkpoint();
                    self.editing = Some(key);
                }
                let text = text.clone();
                let ty = self.editor.selected_type();
                self.editor
                    .mutate(|d| apply_field(d, sel.as_deref(), ty, field, &text));
                cx.notify();
            }
        }
    }

    /// Pushes document values into the text fields that aren't being typed into.
    fn sync_inputs(&mut self, window: &Window, cx: &mut Context<Self>) {
        let doc = &self.editor.doc;
        let sel = self.editor.selection.as_deref();
        let ty = self.editor.selected_type();
        let values = field_values(doc, sel, ty);
        let theme = self.t();
        for (field, input) in self.inputs.all() {
            let value = values
                .iter()
                .find(|(f, _)| *f == field)
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            input.update(cx, |i, cx| {
                i.theme = theme;
                if !i.is_focused(window) {
                    i.set_text(&value, cx);
                }
            });
        }
    }

    /// One undoable change to the selected element.
    fn edit_selected(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Diagram, &str)) {
        let Some(id) = self.editor.selection.clone() else {
            return;
        };
        self.editing = None;
        self.editor.edit(|d| f(d, &id));
        cx.notify();
    }

    fn set_tool(&mut self, tool: Tool, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.set_tool(tool);
        window.focus(&self.canvas_focus, cx);
        cx.notify();
    }

    fn set_theme(&mut self, theme: ThemeName, cx: &mut Context<Self>) {
        self.theme = theme;
        self.settings.theme = Some(theme.theme().slug.to_owned());
        self.settings.save();
        cx.notify();
    }

    // ------------------------------------------------------------ canvas input

    fn local(&self, p: gpui::Point<Pixels>) -> [f64; 2] {
        [
            f64::from(p.x - self.canvas_origin.x),
            f64::from(p.y - self.canvas_origin.y),
        ]
    }

    fn canvas_mouse_down(
        &mut self,
        ev: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu = None;
        window.focus(&self.canvas_focus, cx);
        let button = match ev.button {
            MouseButton::Left => Button::Left,
            MouseButton::Middle => Button::Middle,
            _ => Button::Right,
        };
        let before = self.editor.selection.clone();
        let out = self.editor.pointer_down(
            self.local(ev.position),
            button,
            mods(ev.modifiers),
            ev.click_count,
        );
        if before != self.editor.selection {
            self.editing = None;
            self.hub.focus = None;
        }
        if out.focus_label {
            self.sync_inputs(window, cx);
            let label = self.inputs.label.clone();
            label.update(cx, TextInput::select_all_text);
            window.focus(&label.focus_handle(cx), cx);
        }
        cx.notify();
    }

    fn canvas_scroll(&mut self, ev: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let d = match ev.delta {
            ScrollDelta::Pixels(p) => [f64::from(p.x), f64::from(p.y)],
            ScrollDelta::Lines(l) => [f64::from(l.x) * 40.0, f64::from(l.y) * 40.0],
        };
        let zoom = ev.modifiers.secondary() || ev.modifiers.control;
        // Shift turns a vertical wheel into horizontal panning.
        let d = if ev.modifiers.shift && d[0] == 0.0 && !zoom {
            [d[1], 0.0]
        } else {
            d
        };
        self.editor.wheel(self.local(ev.position), d, zoom);
        cx.notify();
    }

    fn scene(&self) -> Scene {
        let e = &self.editor;
        Scene {
            doc: e.doc.clone(),
            camera: e.camera,
            selection: e.selection.clone(),
            handles: e.handles(),
            drag: e.drag.clone(),
            connect_origin: match &e.drag {
                Some(crate::state::Drag::Connect { from, .. }) => e.connect_origin(from),
                _ => None,
            },
            line_draft: e.line_draft.clone(),
            cursor: e.cursor,
            theme: self.t(),
        }
    }

    fn render_canvas(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let paint_entity = entity.clone();
        let cursor = if self.editor.space || self.editor.tool == Tool::Hand {
            CursorStyle::OpenHand
        } else if matches!(self.editor.drag, Some(crate::state::Drag::Pan { .. })) {
            CursorStyle::ClosedHand
        } else if self.editor.tool == Tool::Select {
            CursorStyle::Arrow
        } else {
            CursorStyle::Crosshair
        };
        div()
            .id("canvas")
            .key_context(CANVAS)
            .track_focus(&self.canvas_focus)
            .relative()
            .flex_1()
            .h_full()
            .overflow_hidden()
            .cursor(cursor)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::canvas_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::canvas_mouse_down))
            .on_scroll_wheel(cx.listener(Self::canvas_scroll))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                if ev.keystroke.key == "space" && !this.editor.space {
                    this.editor.space = true;
                    cx.notify();
                }
            }))
            .on_key_up(cx.listener(|this, ev: &KeyUpEvent, _, cx| {
                if ev.keystroke.key == "space" {
                    this.editor.space = false;
                    cx.notify();
                }
            }))
            .child(
                canvas(
                    move |bounds, _, cx| {
                        entity.update(cx, |app, _| {
                            app.canvas_origin = bounds.origin;
                            app.editor.viewport =
                                [f64::from(bounds.size.width), f64::from(bounds.size.height)];
                            if app.needs_fit && bounds.size.width > px(0.) {
                                app.editor.fit();
                                app.needs_fit = false;
                            }
                            app.scene()
                        })
                    },
                    move |bounds, scene, window, cx| {
                        paint::paint(&scene, bounds, window, cx);
                        // Drags keep tracking the pointer outside the canvas.
                        let e = paint_entity.clone();
                        window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                            if phase != gpui::DispatchPhase::Bubble {
                                return;
                            }
                            e.update(cx, |app, cx| {
                                let tracking =
                                    app.editor.drag.is_some() || app.editor.line_draft.is_some();
                                if tracking
                                    && app
                                        .editor
                                        .pointer_move(app.local(ev.position), mods(ev.modifiers))
                                {
                                    cx.notify();
                                }
                            });
                        });
                        let e = paint_entity.clone();
                        window.on_mouse_event(move |ev: &MouseUpEvent, phase, _, cx| {
                            if phase != gpui::DispatchPhase::Bubble {
                                return;
                            }
                            e.update(cx, |app, cx| {
                                if app.editor.drag.is_some() {
                                    app.editor.pointer_up(app.local(ev.position));
                                    cx.notify();
                                }
                            });
                        });
                    },
                )
                .size_full(),
            )
    }
}

fn mods(m: gpui::Modifiers) -> Mods {
    Mods {
        shift: m.shift,
        alt: m.alt,
    }
}

fn slug(title: &str) -> String {
    let s: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_owned();
    if s.is_empty() { "diagram".into() } else { s }
}

fn field_values(doc: &Diagram, sel: Option<&str>, ty: Option<ElementType>) -> Vec<(Field, String)> {
    let mut v = vec![
        (Field::Title, doc.title.clone()),
        (Field::Description, doc.description.clone()),
    ];
    let Some(id) = sel else { return v };
    v.push((Field::Id, id.to_owned()));
    let (label, color) = match ty {
        Some(ElementType::Node) => doc
            .nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| (n.label.clone(), n.color.clone())),
        Some(ElementType::Zone) => doc
            .zones
            .iter()
            .find(|z| z.id == id)
            .map(|z| (z.label.clone(), z.color.clone())),
        Some(ElementType::Edge) => doc
            .edges
            .iter()
            .find(|e| e.id == id)
            .map(|e| (e.label.clone(), e.color.clone())),
        Some(ElementType::Line) => doc
            .lines
            .iter()
            .find(|l| l.id == id)
            .map(|l| (String::new(), l.color.clone())),
        Some(ElementType::Note) => doc
            .notes
            .iter()
            .find(|n| n.id == id)
            .map(|n| (note_to_field(&n.text), n.color.clone())),
        None => None,
    }
    .unwrap_or_default();
    v.push((Field::Label, label));
    v.push((Field::Color, color.unwrap_or_default()));
    v
}

fn apply_field(
    d: &mut Diagram,
    sel: Option<&str>,
    ty: Option<ElementType>,
    field: Field,
    text: &str,
) {
    match field {
        Field::Title => text.clone_into(&mut d.title),
        Field::Description => text.clone_into(&mut d.description),
        Field::Id => {}
        Field::Label | Field::Color => {
            let Some(id) = sel else { return };
            let color = (!text.is_empty()).then(|| text.to_owned());
            let label = text.to_owned();
            match ty {
                Some(ElementType::Node) => {
                    if let Some(n) = d.node_mut(id) {
                        if field == Field::Label {
                            n.label = label;
                        } else {
                            n.color = color;
                        }
                    }
                }
                Some(ElementType::Zone) => {
                    if let Some(z) = d.zone_mut(id) {
                        if field == Field::Label {
                            z.label = label;
                        } else {
                            z.color = color;
                        }
                    }
                }
                Some(ElementType::Edge) => {
                    if let Some(e) = d.edge_mut(id) {
                        if field == Field::Label {
                            e.label = label;
                        } else {
                            e.color = color;
                        }
                    }
                }
                Some(ElementType::Line) => {
                    if let Some(l) = d.line_mut(id)
                        && field == Field::Color
                    {
                        l.color = color;
                    }
                }
                Some(ElementType::Note) => {
                    if let Some(n) = d.note_mut(id) {
                        if field == Field::Label {
                            n.text = field_to_note(text);
                        } else {
                            n.color = color;
                        }
                    }
                }
                None => {}
            }
        }
    }
}

// ---------------------------------------------------------------- widgets

/// Colours shared by the chrome widgets.
#[derive(Clone, Copy)]
struct Ui {
    t: &'static Theme,
}

impl Ui {
    fn ink(self) -> Hsla {
        hsla(self.t.ink)
    }
    fn muted(self) -> Hsla {
        hsla(self.t.muted)
    }
    fn line(self) -> Hsla {
        hsla(self.t.line)
    }
    fn panel(self) -> Hsla {
        hsla(self.t.panel)
    }
    fn accent(self) -> Hsla {
        hsla(self.t.accent)
    }
    fn hover(self) -> Hsla {
        hsla(mix(self.t.panel, self.t.ink, 0.08))
    }
    fn active_bg(self) -> Hsla {
        hsla(mix(self.t.panel, self.t.accent, 0.16))
    }

    fn icon(name: &str, size: f32, color: Hsla) -> impl IntoElement {
        svg()
            .path(SharedString::from(format!("ui/{name}.svg")))
            .size(px(size))
            .flex_none()
            .text_color(color)
    }

    /// A flat button; stops the press from reaching the title bar's window drag.
    fn button(self, id: impl Into<SharedString>, active: bool) -> gpui::Stateful<gpui::Div> {
        let hover = self.hover();
        let id: SharedString = id.into();
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(px(6.))
            .h(px(26.))
            .px(px(8.))
            .rounded(px(6.))
            .text_size(px(12.5))
            .text_color(if active { self.accent() } else { self.ink() })
            .when(active, |d| d.bg(self.active_bg()))
            .hover(move |s| s.bg(hover))
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    }

    fn section(self, title: &str) -> impl IntoElement {
        div()
            .pt(px(12.))
            .pb(px(4.))
            .text_size(px(10.5))
            .font_weight(gpui::FontWeight(650.))
            .text_color(self.muted())
            .child(title.to_uppercase())
    }

    fn row(self, label: &str, child: impl IntoElement) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .pt(px(8.))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(self.muted())
                    .child(label.to_owned()),
            )
            .child(child)
    }
}

impl InfraPlot {
    /// A row of mutually exclusive options.
    fn segmented<T: Copy + PartialEq + 'static>(
        &self,
        id: &str,
        options: &[(T, &str)],
        current: T,
        cx: &mut Context<Self>,
        apply: fn(&mut Diagram, &str, T),
    ) -> impl IntoElement {
        let ui = Ui { t: self.t() };
        div()
            .flex()
            .flex_wrap()
            .gap(px(2.))
            .p(px(2.))
            .rounded(px(7.))
            .bg(hsla(self.t().paper))
            .border_1()
            .border_color(ui.line())
            .children(options.iter().enumerate().map(|(i, &(value, label))| {
                ui.button(SharedString::from(format!("{id}-{i}")), value == current)
                    .h(px(22.))
                    .px(px(7.))
                    .text_size(px(11.5))
                    .child(label.to_owned())
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.edit_selected(cx, |d, sel| apply(d, sel, value));
                    }))
            }))
    }

    fn swatches(&self, cx: &mut Context<Self>) -> impl IntoElement {
        const SWATCHES: [&str; 10] = [
            "#a5d8ff", "#b2f2bb", "#ffec99", "#ffc9c9", "#d0bfff", "#99e9f2", "#ffd8a8", "#dee2e6",
            "#4f5bd5", "#e03131",
        ];
        let ui = Ui { t: self.t() };
        div()
            .flex()
            .flex_wrap()
            .gap(px(5.))
            .items_center()
            .children(SWATCHES.iter().enumerate().map(|(i, &c)| {
                div()
                    .id(SharedString::from(format!("swatch-{i}")))
                    .size(px(18.))
                    .rounded(px(5.))
                    .bg(hsla(parse_hex(c).unwrap_or(0)))
                    .border_1()
                    .border_color(ui.line())
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        let ty = this.editor.selected_type();
                        this.edit_selected(cx, |d, sel| {
                            apply_field(d, Some(sel), ty, Field::Color, c);
                        });
                    }))
            }))
            .child(
                ui.button("swatch-reset", false)
                    .h(px(20.))
                    .px(px(6.))
                    .text_size(px(11.))
                    .child("Default")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        let ty = this.editor.selected_type();
                        this.edit_selected(cx, |d, sel| {
                            apply_field(d, Some(sel), ty, Field::Color, "");
                        });
                    })),
            )
    }

    #[allow(clippy::too_many_lines)] // one child per control
    fn render_titlebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = Ui { t: self.t() };
        let e = &self.editor;
        let file = e.path.as_ref().map_or_else(
            || "not saved".to_owned(),
            |p| {
                p.file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
            },
        );
        let maximized = window.is_maximized();
        let client = matches!(window.window_decorations(), Decorations::Client { .. });
        let examples_open = self.menu == Some(Menu::Examples);

        let control = |id: &'static str, icon: &'static str, danger: bool| {
            let hover = if danger {
                hsla(self.t().danger)
            } else {
                ui.hover()
            };
            div()
                .id(id)
                .flex()
                .items_center()
                .justify_center()
                .w(px(40.))
                .h_full()
                .text_color(ui.ink())
                .hover(move |s| s.bg(hover))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(Ui::icon(icon, 15., ui.ink()))
        };

        div()
            .id("titlebar")
            .flex()
            .flex_none()
            .items_center()
            .h(px(40.))
            .pl(px(10.))
            .gap(px(2.))
            .bg(ui.panel())
            .border_b_1()
            .border_color(ui.line())
            // Like Zed: the move starts once the pointer moves with the button down, so a
            // double click still reaches `on_click` (the compositor grabs the pointer).
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, _| this.title_drag = true),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| this.title_drag = false),
            )
            .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, window, _| {
                if this.title_drag {
                    this.title_drag = false;
                    window.start_window_move();
                }
            }))
            .on_click(|ev, window, _| {
                if ev.click_count() == 2 {
                    window.zoom_window();
                } else if ev.is_right_click() {
                    window.show_window_menu(ev.position());
                }
            })
            .child(Ui::icon("logo", 18., ui.accent()))
            .child(
                div()
                    .pl(px(6.))
                    .pr(px(10.))
                    .text_size(px(13.))
                    .font_weight(gpui::FontWeight(650.))
                    .text_color(ui.ink())
                    .child("infra-plot"),
            )
            .child(
                ui.button("new", false).child("New").on_click(
                    cx.listener(|this, _: &ClickEvent, _, cx| this.guard(Pending::New, cx)),
                ),
            )
            .child(
                ui.button("open", false).child("Open…").on_click(
                    cx.listener(|this, _: &ClickEvent, _, cx| this.guard(Pending::Open, cx)),
                ),
            )
            .child(
                div()
                    .relative()
                    .child(
                        ui.button("examples", examples_open)
                            .child("Examples")
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.menu = if examples_open {
                                    None
                                } else {
                                    Some(Menu::Examples)
                                };
                                cx.notify();
                            })),
                    )
                    .when(examples_open, |d| {
                        d.child(gpui::deferred(
                            gpui::anchored().snap_to_window_with_margin(px(8.)).child(
                                div()
                                    .id("examples-menu")
                                    .occlude()
                                    .mt(px(30.))
                                    .p(px(4.))
                                    .min_w(px(180.))
                                    .rounded(px(8.))
                                    .bg(ui.panel())
                                    .border_1()
                                    .border_color(ui.line())
                                    .shadow_lg()
                                    .on_mouse_down_out(cx.listener(
                                        |this, _: &MouseDownEvent, _, cx| {
                                            this.menu = None;
                                            cx.notify();
                                        },
                                    ))
                                    .children(EXAMPLES.iter().enumerate().map(|(i, (name, _))| {
                                        ui.button(SharedString::from(format!("example-{i}")), false)
                                            .child(*name)
                                            .on_click(cx.listener(
                                                move |this, _: &ClickEvent, _, cx| {
                                                    this.guard(Pending::Example(i), cx);
                                                },
                                            ))
                                    })),
                            ),
                        ))
                    }),
            )
            .child(
                div()
                    .relative()
                    .child(
                        ui.button("vault", self.menu == Some(Menu::Vault))
                            .child(Ui::icon("vault", 15., ui.ink()))
                            .child("Vault")
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.toggle_menu(Menu::Vault, window, cx);
                            })),
                    )
                    .when(self.menu == Some(Menu::Vault), |d| {
                        d.child(gpui::deferred(
                            gpui::anchored()
                                .snap_to_window_with_margin(px(8.))
                                .child(self.render_vault_menu(cx)),
                        ))
                    }),
            )
            .child(
                ui.button("save", false)
                    .child("Save")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.save(cx))),
            )
            .child(
                ui.button("save-as", false)
                    .child("Save as…")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.save_as(cx))),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .justify_center()
                    .items_center()
                    .gap(px(8.))
                    .overflow_hidden()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight(600.))
                            .text_color(ui.ink())
                            .child(if e.doc.title.is_empty() {
                                "Untitled diagram".to_owned()
                            } else {
                                e.doc.title.clone()
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(ui.muted())
                            .child(if e.dirty { format!("{file} •") } else { file }),
                    ),
            )
            .child(
                ui.button("undo", false)
                    .child(Ui::icon(
                        "undo",
                        15.,
                        if e.can_undo() { ui.ink() } else { ui.line() },
                    ))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.editor.undo();
                        cx.notify();
                    })),
            )
            .child(
                ui.button("redo", false)
                    .child(Ui::icon(
                        "redo",
                        15.,
                        if e.can_redo() { ui.ink() } else { ui.line() },
                    ))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.editor.redo();
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .relative()
                    .child(
                        ui.button("web", self.menu == Some(Menu::Web))
                            .child(Ui::icon("web", 15., ui.ink()))
                            .child(if self.hub.publishing {
                                "Uploading…"
                            } else {
                                "Open in web"
                            })
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.toggle_menu(Menu::Web, window, cx);
                            })),
                    )
                    .when(self.menu == Some(Menu::Web), |d| {
                        d.child(gpui::deferred(
                            gpui::anchored()
                                .snap_to_window_with_margin(px(8.))
                                .child(self.render_web_menu(cx)),
                        ))
                    }),
            )
            .child(
                ui.button("theme", false)
                    .child(Ui::icon("theme", 15., ui.ink()))
                    .child(self.t().label)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        let next = this.theme.next();
                        this.set_theme(next, cx);
                    })),
            )
            .child(div().w(px(8.)))
            .when(client, |d| {
                d.child(
                    control("minimize", "minimize", false)
                        .on_click(|_, window, _| window.minimize_window()),
                )
                .child(
                    control(
                        "maximize",
                        if maximized { "restore" } else { "maximize" },
                        false,
                    )
                    .on_click(|_, window, _| window.zoom_window()),
                )
                .child(control("close", "close", true).on_click(
                    cx.listener(|this, _: &ClickEvent, _, cx| this.guard(Pending::Quit, cx)),
                ))
            })
    }

    fn render_palette(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = Ui { t: self.t() };
        let tool = self.editor.tool;
        let tools: [(Tool, &str, &str, &str); 6] = [
            (Tool::Select, "select", "Select", "V"),
            (Tool::Hand, "hand", "Pan", "H"),
            (Tool::Connect, "connect", "Connect", "C"),
            (Tool::Line, "line", "Line", "L"),
            (Tool::Note, "note", "Note", "T"),
            (Tool::Zone(ZoneKind::Generic), "zone", "Zone", "R"),
        ];
        let mut list = div()
            .id("palette")
            .flex()
            .flex_col()
            .flex_none()
            .w(px(232.))
            .h_full()
            .px(px(10.))
            .pb(px(12.))
            .overflow_y_scroll()
            .bg(ui.panel())
            .border_r_1()
            .border_color(ui.line())
            .child(ui.section("Tools"))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(2.))
                    .children(tools.into_iter().map(|(t, icon, label, key)| {
                        let active = tool == t || (icon == "zone" && matches!(tool, Tool::Zone(_)));
                        ui.button(SharedString::from(format!("tool-{icon}")), active)
                            .w(px(104.))
                            .child(Ui::icon(
                                icon,
                                15.,
                                if active { ui.accent() } else { ui.ink() },
                            ))
                            .child(div().flex_1().child(label))
                            .child(div().text_size(px(10.5)).text_color(ui.muted()).child(key))
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.set_tool(t, window, cx);
                            }))
                    })),
            )
            .child(ui.section("Zones"))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(4.))
                    .children(ZoneKind::ALL.iter().map(|&k| {
                        let info = k.info();
                        let active = tool == Tool::Zone(k);
                        ui.button(SharedString::from(format!("zone-{}", k.slug())), active)
                            .h(px(22.))
                            .px(px(7.))
                            .text_size(px(11.5))
                            .border_1()
                            .border_color(hsla(mix(
                                parse_hex(info.color).unwrap_or(0),
                                self.t().zone_stroke,
                                0.4,
                            )))
                            .child(info.label)
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.set_tool(Tool::Zone(k), window, cx);
                            }))
                    })),
            );
        for group in NodeGroup::ALL {
            list = list.child(ui.section(group.label())).child(
                div().flex().flex_wrap().gap(px(4.)).children(
                    NodeKind::ALL
                        .iter()
                        .filter(|k| k.info().group == group)
                        .map(|&k| self.kind_tile(k, tool == Tool::Node(k), cx, false)),
                ),
            );
        }
        list
    }

    /// A node kind with its icon; `pick` changes the selected node's kind instead of picking
    /// the placing tool.
    fn kind_tile(
        &self,
        k: NodeKind,
        active: bool,
        cx: &mut Context<Self>,
        pick: bool,
    ) -> AnyElement {
        let t = self.t();
        let ui = Ui { t };
        let info = k.info();
        let color = parse_hex(info.color).unwrap_or(0xdee2e6);
        let tile = t.surface(color);
        let hover = ui.hover();
        div()
            .id(SharedString::from(format!(
                "{}kind-{}",
                if pick { "pick-" } else { "" },
                k.slug()
            )))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(3.))
            .w(px(66.))
            .py(px(6.))
            .rounded(px(8.))
            .cursor_pointer()
            .when(active, |d| d.bg(ui.active_bg()))
            .hover(move |s| s.bg(hover))
            .child(
                div()
                    .relative()
                    .size(px(34.))
                    .rounded(px(8.))
                    .bg(hsla(tile))
                    .border_1()
                    .border_color(hsla(mix(tile, t.ink, 0.28)))
                    .child(
                        svg()
                            .absolute()
                            .top(px(6.))
                            .left(px(6.))
                            .size(px(20.))
                            .path(SharedString::from(format!("icons/{}.body.svg", k.slug())))
                            .text_color(hsla(mix(t.panel, color, 0.14))),
                    )
                    .child(
                        svg()
                            .absolute()
                            .top(px(6.))
                            .left(px(6.))
                            .size(px(20.))
                            .path(SharedString::from(format!("icons/{}.svg", k.slug())))
                            .text_color(hsla(mix(color, t.ink, 0.72))),
                    ),
            )
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(ui.ink())
                    .child(info.label),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                if pick {
                    this.menu = None;
                    this.edit_selected(cx, |d, sel| {
                        if let Some(n) = d.node_mut(sel) {
                            // Keep custom labels; follow the kind if it still had the default.
                            if n.label == n.kind.info().label {
                                k.info().label.clone_into(&mut n.label);
                            }
                            n.kind = k;
                        }
                    });
                } else {
                    this.set_tool(Tool::Node(k), window, cx);
                }
            }))
            .into_any_element()
    }

    #[allow(clippy::too_many_lines)] // one block per element type
    fn render_props(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = Ui { t: self.t() };
        let e = &self.editor;
        let doc = &e.doc;
        let sel = e.selection.clone();
        let ty = e.selected_type();
        let mut panel = div()
            .id("props")
            .flex()
            .flex_col()
            .flex_none()
            .w(px(268.))
            .h_full()
            .px(px(12.))
            .pb(px(16.))
            .overflow_y_scroll()
            .bg(ui.panel())
            .border_l_1()
            .border_color(ui.line())
            .text_color(ui.ink());

        let (Some(id), Some(ty)) = (sel, ty) else {
            match self.hub.focus.clone() {
                Some(Focus::Host(h)) if self.hub.vault.is_some() => {
                    return panel.child(self.render_host(&h, true, cx));
                }
                Some(Focus::Overview) if self.hub.vault.is_some() => {
                    return panel.child(self.render_overview());
                }
                _ => {}
            }
            let e = &self.editor;
            let doc = &e.doc;
            let look = doc.look;
            let stats = format!(
                "{} zones · {} nodes · {} edges · {} lines · {} notes",
                doc.zones.len(),
                doc.nodes.len(),
                doc.edges.len(),
                doc.lines.len(),
                doc.notes.len()
            );
            return panel
                .child(ui.section("Document"))
                .child(ui.row("Title", self.inputs.title.clone()))
                .child(ui.row("Description", self.inputs.description.clone()))
                .child(ui.row(
                    "Look",
                    div()
                        .flex()
                        .gap(px(4.))
                        .children([(Look::Clean, "Clean"), (Look::Sketch, "Sketch")].map(
                            |(l, label)| {
                                ui.button(SharedString::from(format!("look-{label}")), look == l)
                                    .child(label)
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.editing = None;
                                        this.editor.edit(|d| d.look = l);
                                        cx.notify();
                                    }))
                            },
                        )),
                ))
                .when(look == Look::Sketch, |d| {
                    d.child(
                        div()
                            .pt(px(6.))
                            .text_size(px(11.))
                            .text_color(ui.muted())
                            .child("The desktop app draws the sketch look like clean; it is kept in the file for the web editor."),
                    )
                })
                .child(
                    div()
                        .pt(px(14.))
                        .text_size(px(11.))
                        .text_color(ui.muted())
                        .child(stats),
                );
        };

        panel = panel
            .child(ui.section(ty.label()))
            .child(ui.row("Id (Enter to rename)", self.inputs.id.clone()));

        match ty {
            ElementType::Node => {
                let Some(n) = doc.nodes.iter().find(|n| n.id == id) else {
                    return panel;
                };
                let kind = n.kind;
                let hidden = self.hidden_meta(&id);
                let meta: Vec<(String, String)> = n
                    .meta
                    .iter()
                    .filter(|(k, _)| {
                        !hidden.contains(&k.as_str()) && (hidden.is_empty() || k.as_str() != "host")
                    })
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                let picking = self.menu == Some(Menu::NodeKind);
                panel = panel
                    .child(ui.row("Label", self.inputs.label.clone()))
                    .child(
                        ui.row(
                            "Kind",
                            ui.button("kind-picker", picking)
                                .border_1()
                                .border_color(ui.line())
                                .child(kind.info().label)
                                .child(Ui::icon(
                                    if picking {
                                        "chevron-up"
                                    } else {
                                        "chevron-down"
                                    },
                                    12.,
                                    ui.muted(),
                                ))
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.menu = if picking { None } else { Some(Menu::NodeKind) };
                                    cx.notify();
                                })),
                        ),
                    )
                    .when(picking, |d| {
                        d.child(
                            div().pt(px(6.)).flex().flex_wrap().gap(px(2.)).children(
                                NodeKind::ALL
                                    .iter()
                                    .map(|&k| self.kind_tile(k, k == kind, cx, true)),
                            ),
                        )
                    })
                    .child(ui.row("Colour", self.inputs.color.clone()))
                    .child(div().pt(px(6.)).child(self.swatches(cx)));
                if !meta.is_empty() {
                    panel = panel
                        .child(ui.section("Details"))
                        .children(meta.into_iter().map(|(k, v)| {
                            div()
                                .flex()
                                .gap(px(8.))
                                .py(px(2.))
                                .text_size(px(12.))
                                .child(div().w(px(90.)).text_color(ui.muted()).child(k))
                                .child(div().flex_1().child(v))
                        }));
                }
                if let Some(vault) = self.render_node_vault(&id, cx) {
                    panel = panel.child(vault);
                }
            }
            ElementType::Zone => {
                let Some(z) = doc.zones.iter().find(|z| z.id == id) else {
                    return panel;
                };
                let kind = z.kind;
                let style = z.style.unwrap_or(kind.info().style);
                panel =
                    panel
                        .child(ui.row("Label", self.inputs.label.clone()))
                        .child(ui.row(
                            "Kind",
                            div().flex().flex_wrap().gap(px(3.)).children(
                                ZoneKind::ALL.iter().map(|&k| {
                                    ui.button(
                                        SharedString::from(format!("zk-{}", k.slug())),
                                        k == kind,
                                    )
                                    .h(px(22.))
                                    .px(px(7.))
                                    .text_size(px(11.5))
                                    .child(k.info().label)
                                    .on_click(cx.listener(
                                        move |this, _: &ClickEvent, _, cx| {
                                            this.edit_selected(cx, |d, sel| {
                                                if let Some(z) = d.zone_mut(sel) {
                                                    if z.label == z.kind.info().label {
                                                        k.info().label.clone_into(&mut z.label);
                                                    }
                                                    z.kind = k;
                                                }
                                            });
                                        },
                                    ))
                                }),
                            ),
                        ))
                        .child(ui.row(
                            "Border",
                            self.segmented("zone-style", STYLES, style, cx, |d, sel, s| {
                                if let Some(z) = d.zone_mut(sel) {
                                    z.style = (s != z.kind.info().style).then_some(s);
                                }
                            }),
                        ))
                        .child(ui.row("Colour", self.inputs.color.clone()))
                        .child(div().pt(px(6.)).child(self.swatches(cx)));
            }
            ElementType::Edge => {
                let Some(edge) = doc.edges.iter().find(|x| x.id == id) else {
                    return panel;
                };
                let (style, arrow, route) = (edge.style, edge.arrow, edge.route);
                let bend = infraplot_model::geometry::bend_of(edge);
                let ends = format!("{} → {}", edge.from, edge.to);
                panel = panel
                    .child(
                        div()
                            .pt(px(4.))
                            .text_size(px(12.))
                            .text_color(ui.muted())
                            .child(ends),
                    )
                    .child(ui.row("Label", self.inputs.label.clone()))
                    .child(ui.row(
                        "Style",
                        self.segmented("edge-style", STYLES, style, cx, |d, sel, s| {
                            if let Some(e) = d.edge_mut(sel) {
                                e.style = s;
                            }
                        }),
                    ))
                    .child(ui.row(
                        "Arrow",
                        self.segmented("edge-arrow", ARROWS, arrow, cx, |d, sel, a| {
                            if let Some(e) = d.edge_mut(sel) {
                                e.arrow = a;
                            }
                        }),
                    ))
                    .child(ui.row(
                        "Route",
                        self.segmented(
                            "edge-route",
                            &[
                                (Route::Straight, "Straight"),
                                (Route::Orthogonal, "Right angles"),
                            ],
                            route,
                            cx,
                            |d, sel, r| {
                                if let Some(e) = d.edge_mut(sel) {
                                    e.route = r;
                                }
                            },
                        ),
                    ))
                    .when(route == Route::Orthogonal, |d| {
                        let steps = [(0, "0"), (25, "¼"), (50, "½"), (75, "¾"), (100, "1")];
                        #[allow(clippy::cast_possible_truncation)] // 0..=100
                        let cur = (bend * 100.0).round() as i32;
                        d.child(ui.row(
                            "Bend (or drag the handle)",
                            self.segmented("edge-bend", &steps, cur, cx, |d, sel, b| {
                                if let Some(e) = d.edge_mut(sel) {
                                    e.bend = (b != 50).then(|| f64::from(b) / 100.0);
                                }
                            }),
                        ))
                    })
                    .child(ui.row("Colour", self.inputs.color.clone()))
                    .child(div().pt(px(6.)).child(self.swatches(cx)));
            }
            ElementType::Line => {
                let Some(l) = doc.lines.iter().find(|x| x.id == id) else {
                    return panel;
                };
                let (style, arrow, n) = (l.style, l.arrow, l.points.len());
                panel = panel
                    .child(
                        div()
                            .pt(px(4.))
                            .text_size(px(12.))
                            .text_color(ui.muted())
                            .child(format!("{n} points · drag the handles to reshape")),
                    )
                    .child(ui.row(
                        "Style",
                        self.segmented("line-style", STYLES, style, cx, |d, sel, s| {
                            if let Some(l) = d.line_mut(sel) {
                                l.style = s;
                            }
                        }),
                    ))
                    .child(ui.row(
                        "Arrow",
                        self.segmented("line-arrow", ARROWS, arrow, cx, |d, sel, a| {
                            if let Some(l) = d.line_mut(sel) {
                                l.arrow = a;
                            }
                        }),
                    ))
                    .child(ui.row("Colour", self.inputs.color.clone()))
                    .child(div().pt(px(6.)).child(self.swatches(cx)));
            }
            ElementType::Note => {
                let Some(n) = doc.notes.iter().find(|x| x.id == id) else {
                    return panel;
                };
                let size = n.size;
                panel = panel
                    .child(ui.row("Text (\\n for a new line)", self.inputs.label.clone()))
                    .child(ui.row(
                        "Size",
                        self.segmented(
                            "note-size",
                            &[
                                (TextSize::S, "S"),
                                (TextSize::M, "M"),
                                (TextSize::L, "L"),
                                (TextSize::Xl, "XL"),
                            ],
                            size,
                            cx,
                            |d, sel, s| {
                                if let Some(n) = d.note_mut(sel) {
                                    n.size = s;
                                }
                            },
                        ),
                    ))
                    .child(ui.row("Colour", self.inputs.color.clone()))
                    .child(div().pt(px(6.)).child(self.swatches(cx)));
            }
        }

        panel.child(
            div()
                .flex()
                .gap(px(6.))
                .pt(px(16.))
                .when(ty != ElementType::Edge, |d| {
                    d.child(
                        ui.button("duplicate", false)
                            .border_1()
                            .border_color(ui.line())
                            .child("Duplicate")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.editor.duplicate_selection();
                                cx.notify();
                            })),
                    )
                })
                .child(
                    ui.button("delete", false)
                        .border_1()
                        .border_color(ui.line())
                        .text_color(hsla(self.t().danger))
                        .child("Delete")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.editor.delete_selection();
                            cx.notify();
                        })),
                ),
        )
    }

    fn render_status(&self) -> impl IntoElement {
        let ui = Ui { t: self.t() };
        let e = &self.editor;
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(14.))
            .h(px(26.))
            .px(px(12.))
            .bg(ui.panel())
            .border_t_1()
            .border_color(ui.line())
            .text_size(px(11.5))
            .text_color(ui.muted())
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .child(match &self.hub.scan {
                        Some(p) => format!(
                            "Scanning {} · {:.0}% · {} hosts found",
                            self.hub.scan_label,
                            p.fraction() * 100.0,
                            p.found.load(std::sync::atomic::Ordering::Relaxed)
                        ),
                        None => e.tool.hint().to_owned(),
                    }),
            )
            .when_some(self.toast.as_ref(), |d, t| {
                d.child(
                    div()
                        .max_w(px(640.))
                        .overflow_hidden()
                        .text_color(if t.error {
                            hsla(self.t().danger)
                        } else {
                            ui.accent()
                        })
                        .child(t.text.clone()),
                )
            })
            .child(format!("{:.0}%", e.camera.zoom * 100.0))
    }

    fn render_confirm(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = Ui { t: self.t() };
        let pending = self.confirm.clone();
        div()
            .id("confirm-backdrop")
            .occlude()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.35))
            .child(
                div()
                    .w(px(380.))
                    .p(px(18.))
                    .rounded(px(12.))
                    .bg(ui.panel())
                    .border_1()
                    .border_color(ui.line())
                    .shadow_lg()
                    .text_color(ui.ink())
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight(600.))
                            .child("Save changes?"),
                    )
                    .child(
                        div()
                            .pt(px(6.))
                            .text_size(px(12.5))
                            .text_color(ui.muted())
                            .child(format!("“{}” has unsaved changes.", self.editor.doc.title)),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(6.))
                            .pt(px(16.))
                            .child(ui.button("confirm-cancel", false).child("Cancel").on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.confirm = None;
                                    cx.notify();
                                }),
                            ))
                            .child(
                                ui.button("confirm-discard", false)
                                    .child("Don't save")
                                    .on_click(cx.listener({
                                        let pending = pending.clone();
                                        move |this, _: &ClickEvent, _, cx| {
                                            if let Some(p) = pending.clone() {
                                                this.run_pending(p, cx);
                                            }
                                        }
                                    })),
                            )
                            .child(ui.button("confirm-save", true).child("Save").on_click(
                                cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.confirm = None;
                                    if this.editor.path.is_some() {
                                        this.save(cx);
                                        if !this.editor.dirty
                                            && let Some(p) = pending.clone()
                                        {
                                            this.run_pending(p, cx);
                                        }
                                    } else {
                                        // Pick a file first; the user repeats the action after.
                                        this.save_as(cx);
                                    }
                                }),
                            )),
                    ),
            )
    }
}

const STYLES: &[(StrokeStyle, &str)] = &[
    (StrokeStyle::Solid, "Solid"),
    (StrokeStyle::Dashed, "Dashed"),
    (StrokeStyle::Dotted, "Dotted"),
];

const ARROWS: &[(Arrow, &str)] = &[
    (Arrow::None, "None"),
    (Arrow::End, "End"),
    (Arrow::Start, "Start"),
    (Arrow::Both, "Both"),
];

/// Border width of the invisible resize grips around a client-decorated window.
const RESIZE_GRIP: Pixels = px(6.);

fn resize_edge(pos: gpui::Point<Pixels>, size: Size<Pixels>) -> Option<ResizeEdge> {
    let g = RESIZE_GRIP;
    let (left, right) = (pos.x < g, pos.x > size.width - g);
    let (top, bottom) = (pos.y < g, pos.y > size.height - g);
    Some(match (top, bottom, left, right) {
        (true, _, true, _) => ResizeEdge::TopLeft,
        (true, _, _, true) => ResizeEdge::TopRight,
        (_, true, true, _) => ResizeEdge::BottomLeft,
        (_, true, _, true) => ResizeEdge::BottomRight,
        (true, ..) => ResizeEdge::Top,
        (_, true, ..) => ResizeEdge::Bottom,
        (_, _, true, _) => ResizeEdge::Left,
        (_, _, _, true) => ResizeEdge::Right,
        _ => return None,
    })
}

impl Render for InfraPlot {
    #[allow(clippy::too_many_lines)] // the action table
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_inputs(window, cx);
        self.sync_hub(window, cx);
        let title = format!(
            "{}{} — infra-plot",
            if self.editor.dirty { "• " } else { "" },
            self.editor.doc.title
        );
        if title != self.window_title {
            window.set_window_title(&title);
            self.window_title = title;
        }
        let ui = Ui { t: self.t() };
        let decorations = window.window_decorations();
        let tiled = match decorations {
            Decorations::Client { tiling } => tiling.is_tiled(),
            Decorations::Server => true,
        };
        let client = matches!(decorations, Decorations::Client { .. });

        let content = div()
            .id("root")
            .key_context(EDITOR)
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .bg(hsla(self.t().paper))
            .font_family(paint::FONT_FAMILY)
            .text_color(ui.ink())
            .when(client && !tiled, |d| {
                d.rounded(px(10.)).border_1().border_color(hsla(mix(
                    self.t().line,
                    self.t().ink,
                    0.15,
                )))
            })
            .on_action(cx.listener(|this, _: &NewFile, _, cx| this.guard(Pending::New, cx)))
            .on_action(cx.listener(|this, _: &OpenFile, _, cx| this.guard(Pending::Open, cx)))
            .on_action(cx.listener(|this, _: &Save, _, cx| this.save(cx)))
            .on_action(cx.listener(|this, _: &SaveAs, _, cx| this.save_as(cx)))
            .on_action(cx.listener(|this, _: &Quit, _, cx| this.guard(Pending::Quit, cx)))
            .on_action(cx.listener(|this, _: &Undo, _, cx| {
                this.editing = None;
                this.editor.undo();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Redo, _, cx| {
                this.editing = None;
                this.editor.redo();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Duplicate, _, cx| {
                this.editor.duplicate_selection();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &DeleteSelection, _, cx| {
                this.editor.delete_selection();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Cancel, window, cx| {
                let dialog = std::mem::take(&mut this.hub.scan_dialog);
                if this.confirm.take().is_none() && this.menu.take().is_none() && !dialog {
                    if this.canvas_focus.is_focused(window) {
                        this.editor.cancel();
                    } else {
                        // Leave the text field, back to the canvas.
                        window.focus(&this.canvas_focus, cx);
                    }
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FitView, _, cx| {
                this.editor.fit();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| {
                this.editor.zoom_by(1.2);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| {
                this.editor.zoom_by(1.0 / 1.2);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FinishLine, _, cx| {
                this.editor.finish_line(true);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &CycleTheme, _, cx| {
                let next = this.theme.next();
                this.set_theme(next, cx);
            }))
            .on_action(
                cx.listener(|this, _: &ToolSelect, w, cx| this.set_tool(Tool::Select, w, cx)),
            )
            .on_action(cx.listener(|this, _: &ToolHand, w, cx| this.set_tool(Tool::Hand, w, cx)))
            .on_action(cx.listener(|this, _: &ToolZone, w, cx| {
                this.set_tool(Tool::Zone(ZoneKind::Generic), w, cx);
            }))
            .on_action(
                cx.listener(|this, _: &ToolConnect, w, cx| this.set_tool(Tool::Connect, w, cx)),
            )
            .on_action(cx.listener(|this, _: &ToolLine, w, cx| this.set_tool(Tool::Line, w, cx)))
            .on_action(cx.listener(|this, _: &ToolNote, w, cx| this.set_tool(Tool::Note, w, cx)))
            .child(self.render_titlebar(window, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_sidebar(cx))
                    .child(self.render_canvas(cx))
                    .child(self.render_props(cx)),
            )
            .child(self.render_status())
            .when(self.hub.scan_dialog, |d| {
                d.child(self.render_scan_dialog(cx))
            })
            .when(self.confirm.is_some(), |d| d.child(self.render_confirm(cx)));

        // Client-side decorations (Wayland): a transparent margin for the shadow, and resize
        // grips along the window border.
        let inset = if client && !tiled { px(8.) } else { px(0.) };
        window.set_client_inset(inset);
        div()
            .id("window")
            .size_full()
            .when(client && !tiled, |d| {
                d.p(inset)
                    .child(
                        canvas(
                            |_bounds, window, _| {
                                window.insert_hitbox(
                                    gpui::Bounds::new(
                                        point(px(0.), px(0.)),
                                        window.window_bounds().get_bounds().size,
                                    ),
                                    HitboxBehavior::Normal,
                                )
                            },
                            move |_bounds, hitbox, window, _| {
                                let size = window.window_bounds().get_bounds().size;
                                let Some(edge) = resize_edge(window.mouse_position(), size) else {
                                    return;
                                };
                                window.set_cursor_style(
                                    match edge {
                                        ResizeEdge::Top | ResizeEdge::Bottom => {
                                            CursorStyle::ResizeUpDown
                                        }
                                        ResizeEdge::Left | ResizeEdge::Right => {
                                            CursorStyle::ResizeLeftRight
                                        }
                                        ResizeEdge::TopLeft | ResizeEdge::BottomRight => {
                                            CursorStyle::ResizeUpLeftDownRight
                                        }
                                        ResizeEdge::TopRight | ResizeEdge::BottomLeft => {
                                            CursorStyle::ResizeUpRightDownLeft
                                        }
                                    },
                                    &hitbox,
                                );
                            },
                        )
                        .size_full()
                        .absolute(),
                    )
                    .on_mouse_move(|_, window, _| window.refresh())
                    .on_mouse_down(MouseButton::Left, |ev, window, _| {
                        let size = window.window_bounds().get_bounds().size;
                        if let Some(edge) = resize_edge(ev.position, size) {
                            window.start_window_resize(edge);
                        }
                    })
            })
            .child(
                content
                    .when(client && !tiled, |d| {
                        d.shadow(vec![
                            gpui::BoxShadow::new(px(0.), px(2.), gpui::black().opacity(0.3))
                                .blur_radius(inset),
                        ])
                    })
                    .on_mouse_move(|_, _, cx| cx.stop_propagation()),
            )
    }
}

impl Focusable for InfraPlot {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.canvas_focus.clone()
    }
}
