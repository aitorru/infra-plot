//! The network vault side of the editor: open or create a vault, scan the network, import
//! nmap reports, browse hosts and diagrams, write each host's notes, and publish the
//! current diagram to the web editor.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use gpui::{
    AnyElement, ClickEvent, Context, Entity, Focusable, MouseDownEvent, PathPromptOptions,
    SharedString, Subscription, Window, div, prelude::*, px,
};
use infraplot_model::Diagram;
use infraplot_model::ops::ElementType;
use infraplot_vault::graph::{HOST_KEY, SCAN_KEYS, SyncReport, node_for};
use infraplot_vault::{Host, Progress, ScanResult, Vault, net, nmap, scan};

use super::{InfraPlot, Menu, Pending, Ui};
use crate::input::{InputEvent, TextInput};
use crate::textarea::TextArea;
use crate::theme::{Theme, hsla, mix};
use crate::web;

/// What the notes editor is showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Focus {
    /// The vault's `README.md`.
    Overview,
    /// `notes/<host>.md`.
    Host(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideTab {
    Vault,
    Shapes,
}

pub struct Hub {
    pub vault: Option<Vault>,
    pub tab: SideTab,
    /// Shown in the properties panel when nothing is selected.
    pub focus: Option<Focus>,
    /// What `note` currently holds.
    note_target: Option<Focus>,
    pub scan: Option<Arc<Progress>>,
    pub scan_label: String,
    pub scan_dialog: bool,
    pub publishing: bool,
    pub filter: Entity<TextInput>,
    pub targets: Entity<TextInput>,
    pub note: Entity<TextArea>,
    pub web_url: Entity<TextInput>,
}

impl Hub {
    pub fn new(
        theme: &'static Theme,
        web_url: &str,
        cx: &mut Context<InfraPlot>,
        subs: &mut Vec<Subscription>,
    ) -> Self {
        let filter = cx.new(|cx| TextInput::new("Filter hosts", theme, cx));
        subs.push(cx.subscribe(&filter, |_, _, _: &InputEvent, cx| cx.notify()));
        let targets = cx.new(|cx| TextInput::new("192.168.1.0/24, 10.0.0.0/24", theme, cx));
        subs.push(cx.subscribe(&targets, |this, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Submitted) {
                this.start_scan(cx);
            }
        }));
        let note = cx.new(|cx| TextArea::new("Write Markdown notes…", theme, cx));
        subs.push(cx.subscribe(&note, |this, _, ev: &InputEvent, cx| {
            if let InputEvent::Changed(text) = ev {
                this.save_note(text, cx);
            }
        }));
        let url = cx.new(|cx| {
            let mut i = TextInput::new(web::DEFAULT_URL, theme, cx);
            i.set_text(web_url, cx);
            i
        });
        subs.push(cx.subscribe(&url, |this, _, ev: &InputEvent, cx| match ev {
            InputEvent::Changed(text) => {
                this.settings.web_url = Some(text.trim().to_owned()).filter(|s| !s.is_empty());
                this.settings.save();
            }
            InputEvent::Submitted => this.publish_web(cx),
        }));
        Self {
            vault: None,
            tab: SideTab::Shapes,
            focus: None,
            note_target: None,
            scan: None,
            scan_label: String::new(),
            scan_dialog: false,
            publishing: false,
            filter,
            targets,
            note,
            web_url: url,
        }
    }
}

/// `● up`, `○ down`, `◆ manual` colours.
fn status_color(t: &Theme, h: &Host) -> u32 {
    if h.manual {
        t.accent
    } else if h.up {
        0x2f9e44
    } else {
        t.muted
    }
}

fn status_label(h: &Host) -> &'static str {
    if h.manual {
        "documented by hand"
    } else if h.up {
        "up"
    } else {
        "down (did not answer the last scan)"
    }
}

impl InfraPlot {
    // ------------------------------------------------------------ vault lifecycle

    /// Opens the vault in `dir` and its network diagram. The caller has already dealt with
    /// unsaved changes.
    pub fn open_vault(&mut self, dir: &Path, cx: &mut Context<Self>) {
        let dir = Vault::find(dir).unwrap_or_else(|| dir.to_path_buf());
        match Vault::open(&dir) {
            Ok(v) => {
                let diagram = v.network_diagram();
                self.settings.vault = Some(dir.to_string_lossy().into_owned());
                self.settings.save();
                self.notify_msg(format!("Opened vault “{}”", v.config.name), false, cx);
                self.hub.vault = Some(v);
                self.hub.tab = SideTab::Vault;
                self.hub.focus = Some(Focus::Overview);
                self.hub.note_target = None;
                let current = self.editor.path.as_deref().and_then(Vault::find);
                if current != Vault::find(&dir) {
                    self.open_path(&diagram, cx);
                }
            }
            Err(e) => self.notify_msg(e.to_string(), true, cx),
        }
        cx.notify();
    }

    fn create_vault(&mut self, dir: &Path, cx: &mut Context<Self>) {
        if Vault::is_vault(dir) {
            self.open_vault(dir, cx);
            return;
        }
        match Vault::init(dir, None, None) {
            Ok(v) => {
                let nets: Vec<String> = v
                    .config
                    .networks
                    .iter()
                    .map(|n| n.cidr.to_string())
                    .collect();
                self.open_vault(dir, cx);
                if nets.is_empty() {
                    self.notify_msg(
                        "Vault created. Scan… lets you enter the networks to document.",
                        false,
                        cx,
                    );
                } else {
                    self.notify_msg(
                        format!(
                            "Vault created for {}. Run Scan… to fill it.",
                            nets.join(", ")
                        ),
                        false,
                        cx,
                    );
                }
            }
            Err(e) => self.notify_msg(e.to_string(), true, cx),
        }
    }

    pub fn close_vault(&mut self, cx: &mut Context<Self>) {
        self.hub.vault = None;
        self.hub.tab = SideTab::Shapes;
        self.hub.focus = None;
        self.hub.note_target = None;
        self.settings.vault = None;
        self.settings.save();
        self.menu = None;
        cx.notify();
    }

    /// Asks for a folder, then opens (or with `create`, creates) a vault there.
    pub fn vault_dialog(create: bool, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(
                if create {
                    "Create vault here"
                } else {
                    "Open vault"
                }
                .into(),
            ),
        });
        cx.spawn(async move |this, cx| {
            let res = rx.await;
            this.update(cx, |app, cx| match res {
                Ok(Ok(Some(paths))) => {
                    if let Some(p) = paths.first() {
                        if create {
                            app.create_vault(p, cx);
                        } else if Vault::find(p).is_some() {
                            app.open_vault(p, cx);
                        } else {
                            app.notify_msg(
                                format!("{} is not a vault: use Vault → New vault…", p.display()),
                                true,
                                cx,
                            );
                        }
                    }
                }
                Ok(Err(e)) => app.notify_msg(format!("No file dialog available: {e}"), true, cx),
                _ => {}
            })
            .ok();
        })
        .detach();
    }

    /// Opens a diagram of the vault and, optionally, selects the node of a host.
    pub fn open_vault_diagram(&mut self, path: &Path, host: Option<&str>, cx: &mut Context<Self>) {
        if self.editor.path.as_deref() != Some(path) {
            self.open_path(path, cx);
        }
        if let Some(h) = host {
            self.select_host(h, cx);
        }
    }

    fn new_vault_diagram(&mut self, cx: &mut Context<Self>) {
        let Some(v) = &self.hub.vault else { return };
        match v.new_diagram("New diagram") {
            Ok(p) => self.guard(Pending::OpenDiagram(p, None), cx),
            Err(e) => self.notify_msg(e.to_string(), true, cx),
        }
    }

    // ------------------------------------------------------------ hosts and notes

    /// The host a node documents.
    fn linked_host(&self, node_id: &str) -> Option<&Host> {
        let v = self.hub.vault.as_ref()?;
        let n = self.editor.doc.nodes.iter().find(|n| n.id == node_id)?;
        v.inventory.get(n.meta.get(HOST_KEY)?)
    }

    /// What the notes editor should show now.
    fn wanted_note(&self) -> Option<Focus> {
        self.hub.vault.as_ref()?;
        match (
            self.editor.selection.as_deref(),
            self.editor.selected_type(),
        ) {
            (Some(id), Some(ElementType::Node)) => {
                self.linked_host(id).map(|h| Focus::Host(h.id.clone()))
            }
            (None, _) => self.hub.focus.clone(),
            _ => None,
        }
    }

    /// Loads the note being shown into the editor when it changes, and re-themes the hub's
    /// fields.
    pub fn sync_hub(&mut self, window: &Window, cx: &mut Context<Self>) {
        let t = self.t();
        for input in [&self.hub.filter, &self.hub.targets, &self.hub.web_url] {
            input.update(cx, |i, _| i.theme = t);
        }
        self.hub.note.update(cx, |n, _| n.theme = t);
        let wanted = self.wanted_note();
        if wanted == self.hub.note_target || self.hub.note.read(cx).is_focused(window) {
            return;
        }
        let text = match (&wanted, &self.hub.vault) {
            (Some(Focus::Overview), Some(v)) => {
                std::fs::read_to_string(v.overview()).unwrap_or_default()
            }
            (Some(Focus::Host(id)), Some(v)) => {
                let text = v.read_note(id);
                match v.inventory.get(id) {
                    Some(h) if text.is_empty() => v.note_template(h),
                    _ => text,
                }
            }
            _ => String::new(),
        };
        self.hub.note.update(cx, |n, cx| n.set_text(&text, cx));
        self.hub.note_target = wanted;
    }

    fn save_note(&mut self, text: &str, cx: &mut Context<Self>) {
        let (Some(target), Some(v)) = (&self.hub.note_target, &self.hub.vault) else {
            return;
        };
        let res = match target {
            Focus::Overview => std::fs::write(v.overview(), text).map_err(|e| e.to_string()),
            Focus::Host(id) => {
                let new = !v.note_path(id).exists();
                v.write_note(id, text)
                    .and_then(|()| if new { v.save_inventory() } else { Ok(()) })
                    .map_err(|e| e.to_string())
            }
        };
        if let Err(e) = res {
            self.notify_msg(format!("Could not save the note: {e}"), true, cx);
        }
    }

    /// Shows a host: selects its node when the open diagram has one.
    fn select_host(&mut self, id: &str, cx: &mut Context<Self>) {
        self.hub.focus = Some(Focus::Host(id.to_owned()));
        self.editing = None;
        if let Some(n) = node_for(&self.editor.doc, id) {
            let (nid, x, y) = (n.id.clone(), n.x, n.y);
            self.editor.selection = Some(nid);
            let cam = &mut self.editor.camera;
            cam.zoom = cam.zoom.max(0.8);
            cam.x = self.editor.viewport[0] / 2.0 - x * cam.zoom;
            cam.y = self.editor.viewport[1] / 2.0 - y * cam.zoom;
        } else {
            self.editor.selection = None;
        }
        cx.notify();
    }

    /// Ties the selected node to a new hand-made host of the inventory.
    fn document_node(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.editor.selection.clone() else {
            return;
        };
        let Some(n) = self.editor.doc.nodes.iter().find(|n| n.id == id) else {
            return;
        };
        let base = if n.label.trim().is_empty() {
            n.id.clone()
        } else {
            n.label.clone()
        };
        let ip = n.meta.get("ip").and_then(|s| s.parse().ok());
        let Some(v) = self.hub.vault.as_mut() else {
            return;
        };
        match v.add_manual_host(&base, ip) {
            Ok(host) => {
                self.editing = None;
                self.editor.edit(|d| {
                    if let Some(n) = d.node_mut(&id) {
                        n.meta.insert(HOST_KEY.into(), host.clone());
                    }
                });
                self.notify_msg(format!("Added “{host}” to the vault inventory"), false, cx);
            }
            Err(e) => self.notify_msg(e.to_string(), true, cx),
        }
        cx.notify();
    }

    // ------------------------------------------------------------ scans

    pub fn open_scan_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(v) = &self.hub.vault else { return };
        let mut nets: Vec<String> = v
            .config
            .networks
            .iter()
            .map(|n| n.cidr.to_string())
            .collect();
        if nets.is_empty() {
            nets = net::local_networks()
                .iter()
                .map(|n| n.cidr.to_string())
                .collect();
        }
        let text = nets.join(", ");
        self.hub.targets.update(cx, |i, cx| i.set_text(&text, cx));
        self.hub.scan_dialog = true;
        self.menu = None;
        cx.notify();
    }

    pub fn start_scan(&mut self, cx: &mut Context<Self>) {
        if self.hub.scan.is_some() || self.hub.vault.is_none() {
            return;
        }
        let targets = match net::parse_list(self.hub.targets.read(cx).text()) {
            Ok(t) if !t.is_empty() => t,
            Ok(_) => {
                self.notify_msg("Enter at least one network, e.g. 192.168.1.0/24", true, cx);
                return;
            }
            Err(e) => {
                self.notify_msg(e.to_string(), true, cx);
                return;
            }
        };
        // Scanning a network makes it part of the vault (and of its diagram's zones).
        let Some(v) = &mut self.hub.vault else { return };
        if let Err(e) = v.adopt_networks(&targets) {
            self.notify_msg(e.to_string(), true, cx);
        }
        let Some(v) = &self.hub.vault else { return };
        let mut opts = v.scan_options();
        opts.targets = targets;
        self.hub.scan_label = opts
            .targets
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        self.hub.scan_dialog = false;
        let progress = Arc::new(Progress::default());
        self.hub.scan = Some(progress.clone());
        let task = cx
            .background_executor()
            .spawn(async move { scan::scan(&opts, &progress) });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |app, cx| {
                app.hub.scan = None;
                match res {
                    Ok(r) => app.apply_scan(&r, cx),
                    Err(e) => {
                        app.notify_msg(e.to_string(), !matches!(e, scan::ScanError::Cancelled), cx);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        // Repaint the progress bar while the scan runs.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let running = this
                    .update(cx, |app, cx| {
                        cx.notify();
                        app.hub.scan.is_some()
                    })
                    .unwrap_or(false);
                if !running {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    pub fn import_nmap_dialog(cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Import nmap XML (nmap -oX)".into()),
        });
        cx.spawn(async move |this, cx| {
            let res = rx.await;
            this.update(cx, |app, cx| {
                if let Ok(Ok(Some(paths))) = res {
                    for p in paths {
                        app.import_nmap(&p, cx);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    fn import_nmap(&mut self, path: &Path, cx: &mut Context<Self>) {
        let parsed = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|xml| nmap::parse(&xml).map_err(|e| e.to_string()));
        match parsed {
            Ok(r) => self.apply_scan(&r, cx),
            Err(e) => self.notify_msg(format!("{}: {e}", path.display()), true, cx),
        }
    }

    /// Records a scan in the vault and updates the network diagram: through the editor
    /// (undoable) when it is the open document, on disk otherwise.
    fn apply_scan(&mut self, r: &ScanResult, cx: &mut Context<Self>) {
        let Some(v) = &mut self.hub.vault else { return };
        let recorded = v.record_scan_only(r).map(|(m, _)| m);
        let net_path = v.network_diagram();
        let merge = match recorded {
            Ok(m) => m,
            Err(e) => {
                self.notify_msg(e.to_string(), true, cx);
                return;
            }
        };
        let synced = if self.editor.path.as_deref() == Some(net_path.as_path()) {
            let mut doc = self.editor.doc.clone();
            let report = self
                .hub
                .vault
                .as_ref()
                .map(|v| v.sync_diagram(&mut doc))
                .unwrap_or_default();
            if doc != self.editor.doc {
                let was_clean = !self.editor.dirty;
                self.editing = None;
                self.editor.edit(|d| *d = doc);
                if was_clean {
                    self.save_to(&net_path, cx);
                }
                self.needs_fit = report.added_zones > 0;
            }
            Ok(report)
        } else {
            self.hub
                .vault
                .as_ref()
                .map_or(Ok(SyncReport::default()), |v| {
                    v.sync_network_diagram().map_err(|e| e.to_string())
                })
        };
        match synced {
            Ok(s) => self.notify_msg(
                format!(
                    "{} {}: {} · {} nodes added to the network diagram",
                    if r.source == "nmap" {
                        "nmap import"
                    } else {
                        "Scan"
                    },
                    r.finished,
                    merge.summary(),
                    s.added_nodes
                ),
                false,
                cx,
            ),
            Err(e) => self.notify_msg(e, true, cx),
        }
        cx.notify();
    }

    // ------------------------------------------------------------ web

    /// Uploads the open diagram to the infra-plot server and opens it in the browser.
    pub fn publish_web(&mut self, cx: &mut Context<Self>) {
        if self.hub.publishing {
            return;
        }
        let base = self.hub.web_url.read(cx).text().trim().to_owned();
        let base = if base.is_empty() {
            web::DEFAULT_URL.to_owned()
        } else {
            base
        };
        let name = self
            .editor
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map_or_else(
                || self.editor.doc.title.clone(),
                |s| s.to_string_lossy().into_owned(),
            );
        // Vault diagrams are prefixed with the vault, so two vaults don't overwrite each
        // other's `network`.
        let name = match &self.hub.vault {
            Some(v)
                if self
                    .editor
                    .path
                    .as_ref()
                    .is_some_and(|p| p.starts_with(&v.root)) =>
            {
                format!("{}-{name}", v.config.name)
            }
            _ => name,
        };
        let id = web::diagram_id(&name);
        let doc: Diagram = self.editor.doc.clone();
        self.hub.publishing = true;
        self.menu = None;
        let task = cx
            .background_executor()
            .spawn(async move { web::publish(&base, &id, &doc) });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |app, cx| {
                app.hub.publishing = false;
                match res {
                    Ok(url) => {
                        cx.open_url(&url);
                        app.notify_msg(format!("Published to {url}"), false, cx);
                    }
                    Err(e) => app.notify_msg(e, true, cx),
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    // ------------------------------------------------------------ rendering

    pub fn render_sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.hub.vault.is_none() {
            return self.render_palette(cx).into_any_element();
        }
        let ui = Ui { t: self.t() };
        let tab = self.hub.tab;
        let tabs = div()
            .flex()
            .flex_none()
            .gap(px(2.))
            .p(px(6.))
            .bg(ui.panel())
            .border_b_1()
            .border_color(ui.line())
            .children(
                [(SideTab::Vault, "Vault"), (SideTab::Shapes, "Shapes")].map(|(t, label)| {
                    ui.button(SharedString::from(format!("tab-{label}")), tab == t)
                        .flex_1()
                        .justify_center()
                        .child(label)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.hub.tab = t;
                            cx.notify();
                        }))
                }),
            );
        let body = if tab == SideTab::Vault {
            self.render_vault_panel(cx).into_any_element()
        } else {
            self.render_palette(cx).into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(232.))
            .h_full()
            .border_r_1()
            .border_color(ui.line())
            .child(tabs)
            .child(div().flex_1().min_h_0().child(body))
            .into_any_element()
    }

    #[allow(clippy::too_many_lines)] // one block per section
    fn render_vault_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.t();
        let ui = Ui { t };
        let Some(v) = &self.hub.vault else {
            return div().id("vault-panel");
        };
        let query = self.hub.filter.read(cx).text().trim().to_lowercase();
        let current = self.editor.path.clone();
        let diagrams = v.diagrams();
        let hosts: Vec<Host> = v
            .inventory
            .hosts
            .iter()
            .filter(|h| h.matches(&query))
            .cloned()
            .collect();
        let total = v.inventory.hosts.len();
        let up = v
            .inventory
            .hosts
            .iter()
            .filter(|h| h.up && !h.manual)
            .count();
        let focus = self.hub.focus.clone();
        let selected_host = self
            .editor
            .selection
            .as_deref()
            .and_then(|s| self.linked_host(s))
            .map(|h| h.id.clone());
        let name = v.config.name.clone();
        let root = std::env::var_os("HOME")
            .and_then(|h| v.root.strip_prefix(h).ok())
            .map_or_else(
                || v.root.display().to_string(),
                |rel| format!("~/{}", rel.display()),
            );
        let scanning = self.hub.scan.clone();

        let row = |id: SharedString, active: bool| {
            ui.button(id, active)
                .flex_none()
                .w_full()
                .h(px(24.))
                .text_size(px(12.))
                .overflow_hidden()
        };

        // A plain column inside the scroller: as flex items of the scroller itself the rows
        // (which clip their overflow) would shrink to fit instead of scrolling.
        let mut panel = div()
            .flex()
            .flex_col()
            .px(px(10.))
            .pb(px(12.))
            .child(
                div()
                    .pt(px(10.))
                    .text_size(px(14.))
                    .font_weight(gpui::FontWeight(650.))
                    .child(name),
            )
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(ui.muted())
                    .overflow_hidden()
                    .child(root),
            )
            .child(
                row(
                    "vault-overview".into(),
                    focus == Some(Focus::Overview) && self.editor.selection.is_none(),
                )
                .mt(px(8.))
                .child(Ui::icon("note", 13., ui.muted()))
                .child("Overview (README.md)")
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.hub.focus = Some(Focus::Overview);
                    this.editor.selection = None;
                    cx.notify();
                })),
            );

        // Scanning.
        panel = panel.child(ui.section("Discovery"));
        if let Some(p) = scanning {
            let frac = p.fraction();
            #[allow(clippy::cast_possible_truncation)] // 0..=100
            let pct = (frac * 100.0).round() as i32;
            panel = panel
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(ui.muted())
                        .child(format!(
                            "Scanning {} · {pct}% · {} hosts",
                            self.hub.scan_label,
                            p.found.load(Ordering::Relaxed)
                        )),
                )
                .child(
                    div()
                        .mt(px(6.))
                        .h(px(6.))
                        .w_full()
                        .rounded(px(3.))
                        .bg(hsla(t.paper))
                        .child(
                            div()
                                .h_full()
                                .rounded(px(3.))
                                .w(gpui::relative({
                                    #[allow(clippy::cast_possible_truncation)] // 0..=1
                                    let f = frac as f32;
                                    f
                                }))
                                .bg(ui.accent()),
                        ),
                )
                .child(
                    ui.button("scan-cancel", false)
                        .mt(px(6.))
                        .border_1()
                        .border_color(ui.line())
                        .child("Cancel")
                        .on_click(move |_: &ClickEvent, _, _| {
                            p.cancel.store(true, Ordering::Relaxed);
                        }),
                );
        } else {
            panel = panel.child(
                div()
                    .flex()
                    .gap(px(4.))
                    .child(
                        ui.button("scan", false)
                            .border_1()
                            .border_color(ui.line())
                            .child(Ui::icon("scan", 14., ui.ink()))
                            .child("Scan…")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.open_scan_dialog(cx);
                            })),
                    )
                    .child(
                        ui.button("import-nmap", false)
                            .border_1()
                            .border_color(ui.line())
                            .child("Import nmap…")
                            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                                Self::import_nmap_dialog(cx);
                            })),
                    ),
            );
        }

        // Diagrams.
        panel = panel
            .child(ui.section("Diagrams"))
            .children(diagrams.into_iter().enumerate().map(|(i, p)| {
                let label = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let active = current.as_ref() == Some(&p);
                row(SharedString::from(format!("diagram-{i}")), active)
                    .child(Ui::icon(
                        if i == 0 { "logo" } else { "zone" },
                        13.,
                        if active { ui.accent() } else { ui.muted() },
                    ))
                    .child(label)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.guard(Pending::OpenDiagram(p.clone(), None), cx);
                    }))
            }));
        panel = panel.child(
            row("diagram-new".into(), false)
                .text_color(ui.muted())
                .child("+ New diagram")
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_vault_diagram(cx))),
        );

        // Hosts.
        panel = panel
            .child(ui.section(&format!("Hosts · {up} up of {total}")))
            .child(self.hub.filter.clone())
            .child(div().h(px(4.)));
        if total == 0 {
            panel = panel.child(
                div()
                    .text_size(px(11.5))
                    .text_color(ui.muted())
                    .child("No hosts yet. Scan the network or import an nmap report."),
            );
        }
        let panel = panel.children(hosts.into_iter().map(|h| {
            let active = selected_host.as_deref() == Some(h.id.as_str())
                || (self.editor.selection.is_none() && focus == Some(Focus::Host(h.id.clone())));
            let id = h.id.clone();
            let name = h.name();
            // Hosts without a name are already listed by their address.
            let ip =
                h.ip.map(|i| i.to_string())
                    .filter(|ip| *ip != name)
                    .unwrap_or_default();
            row(SharedString::from(format!("host-{}", h.id)), active)
                .child(
                    div()
                        .size(px(7.))
                        .flex_none()
                        .rounded(px(4.))
                        .bg(hsla(status_color(t, &h))),
                )
                .child(div().flex_1().overflow_hidden().child(name))
                .child(div().text_size(px(10.5)).text_color(ui.muted()).child(ip))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_host(&id, cx)))
        }));
        div()
            .id("vault-panel")
            .size_full()
            .overflow_y_scroll()
            .bg(ui.panel())
            .child(panel)
    }

    /// Facts and notes of a host, for the properties panel.
    #[allow(clippy::too_many_lines)] // one child per fact
    pub fn render_host(
        &mut self,
        host_id: &str,
        with_title: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.t();
        let ui = Ui { t };
        let Some(v) = &self.hub.vault else {
            return div().into_any_element();
        };
        let Some(h) = v.inventory.get(host_id).cloned() else {
            return div().into_any_element();
        };
        let note_file = format!("notes/{}.md", h.id);
        let on_canvas = node_for(&self.editor.doc, &h.id).is_some();
        let net_diagram = v.network_diagram();
        let fact = |k: &str, v: String| {
            div()
                .flex()
                .flex_none()
                .gap(px(8.))
                .py(px(2.))
                .text_size(px(12.))
                .child(
                    div()
                        .w(px(78.))
                        .flex_none()
                        .text_color(ui.muted())
                        .child(k.to_owned()),
                )
                .child(div().flex_1().overflow_hidden().child(v))
        };
        let mut facts: Vec<(&str, String)> = vec![("Status", status_label(&h).to_owned())];
        if let Some(ip) = h.ip {
            facts.push(("IP", ip.to_string()));
        }
        for (k, val) in [("MAC", &h.mac), ("Vendor", &h.vendor), ("OS", &h.os)] {
            if let Some(val) = val {
                facts.push((k, val.clone()));
            }
        }
        if !h.hostnames.is_empty() {
            facts.push(("Names", h.hostnames.join(", ")));
        }
        if let Some(s) = &h.first_seen {
            facts.push(("First seen", s.replace('T', " ").replace('Z', " UTC")));
        }
        if let Some(s) = &h.last_seen {
            facts.push(("Last seen", s.replace('T', " ").replace('Z', " UTC")));
        }
        let host_name = h.name();
        let id = h.id.clone();
        div()
            .flex()
            .flex_col()
            .when(with_title, |d| {
                d.child(ui.section("Host")).child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .size(px(9.))
                                .rounded(px(5.))
                                .bg(hsla(status_color(t, &h))),
                        )
                        .child(
                            div()
                                .text_size(px(15.))
                                .font_weight(gpui::FontWeight(650.))
                                .child(host_name),
                        ),
                )
            })
            .when(!with_title, |d| {
                d.child(ui.section(&format!("Vault host · {}", h.id)))
            })
            .children(facts.into_iter().map(|(k, v)| fact(k, v)))
            .when(!h.ports.is_empty(), |d| {
                d.child(ui.section(&format!("Open ports · {}", h.ports.len())))
                    .children(h.ports.iter().map(|p| {
                        let what = match (&p.service, &p.product) {
                            (Some(s), Some(pr)) => format!("{s} · {pr}"),
                            (Some(s), None) => s.clone(),
                            (None, Some(pr)) => pr.clone(),
                            (None, None) => String::new(),
                        };
                        fact(&format!("{}/{}", p.port, p.proto), what)
                    }))
            })
            .when(with_title && !on_canvas, |d| {
                d.child(
                    ui.button("show-on-diagram", false)
                        .mt(px(10.))
                        .border_1()
                        .border_color(ui.line())
                        .child("Show in the network diagram")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.guard(
                                Pending::OpenDiagram(net_diagram.clone(), Some(id.clone())),
                                cx,
                            );
                        })),
                )
            })
            .child(ui.section("Notes"))
            .child(
                div()
                    .pb(px(4.))
                    .text_size(px(10.5))
                    .text_color(ui.muted())
                    .child(format!("Markdown, saved as you type to {note_file}")),
            )
            .child(self.hub.note.clone())
            .into_any_element()
    }

    pub fn render_overview(&self) -> AnyElement {
        let ui = Ui { t: self.t() };
        let Some(v) = &self.hub.vault else {
            return div().into_any_element();
        };
        let nets = if v.config.networks.is_empty() {
            "none yet (set them when scanning)".to_owned()
        } else {
            v.config
                .networks
                .iter()
                .map(|n| match n.gateway {
                    Some(g) => format!("{} via {g}", n.cidr),
                    None => n.cidr.to_string(),
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        div()
            .flex()
            .flex_col()
            .child(ui.section("Vault"))
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(gpui::FontWeight(650.))
                    .child(v.config.name.clone()),
            )
            .child(
                div()
                    .pt(px(4.))
                    .text_size(px(12.))
                    .text_color(ui.muted())
                    .child(format!(
                        "{} hosts · networks: {nets}",
                        v.inventory.hosts.len()
                    )),
            )
            .child(ui.section("Overview"))
            .child(
                div()
                    .pb(px(4.))
                    .text_size(px(10.5))
                    .text_color(ui.muted())
                    .child("Markdown, saved as you type to README.md"),
            )
            .child(self.hub.note.clone())
            .into_any_element()
    }

    /// Extra controls for a node in the properties panel: its host, or a way to add it.
    pub fn render_node_vault(
        &mut self,
        node_id: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.hub.vault.as_ref()?;
        if let Some(h) = self.linked_host(node_id) {
            let id = h.id.clone();
            return Some(self.render_host(&id, false, cx));
        }
        let ui = Ui { t: self.t() };
        Some(
            div()
                .flex()
                .flex_col()
                .child(ui.section("Vault"))
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(ui.muted())
                        .child("Not in the vault inventory. Add it to keep notes about it (useful for devices a scan can't see: switches, cloud services, ISP links)."),
                )
                .child(
                    ui.button("document-node", false)
                        .mt(px(6.))
                        .border_1()
                        .border_color(ui.line())
                        .child("Add to the inventory")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.document_node(cx))),
                )
                .into_any_element(),
        )
    }

    /// Meta keys the host section already shows.
    pub fn hidden_meta(&self, node_id: &str) -> &'static [&'static str] {
        if self.linked_host(node_id).is_some() {
            SCAN_KEYS
        } else {
            &[]
        }
    }

    pub fn render_scan_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = Ui { t: self.t() };
        let ports = self
            .hub
            .vault
            .as_ref()
            .map_or(0, |v| v.scan_options().ports.len());
        div()
            .id("scan-backdrop")
            .occlude()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.35))
            .child(
                div()
                    .w(px(460.))
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
                            .child("Scan the network"),
                    )
                    .child(
                        div()
                            .pt(px(6.))
                            .text_size(px(12.))
                            .text_color(ui.muted())
                            .child(format!(
                                "Probes {ports} common TCP ports on every address and reads the ARP cache, without root. Up to {} addresses. For vendors, OS and versions, run nmap and use Import nmap… instead.",
                                scan::MAX_HOSTS
                            )),
                    )
                    .child(ui.row("Networks (comma separated)", self.hub.targets.clone()))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(6.))
                            .pt(px(16.))
                            .child(ui.button("scan-dialog-cancel", false).child("Cancel").on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.hub.scan_dialog = false;
                                    cx.notify();
                                }),
                            ))
                            .child(ui.button("scan-start", true).child("Scan").on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.start_scan(cx)),
                            )),
                    ),
            )
    }

    /// The Vault menu of the title bar.
    pub fn render_vault_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = Ui { t: self.t() };
        let open = self.hub.vault.is_some();
        let item =
            |id: &'static str, label: &'static str| ui.button(id, false).w_full().child(label);
        div()
            .id("vault-menu")
            .occlude()
            .mt(px(30.))
            .p(px(4.))
            .min_w(px(200.))
            .rounded(px(8.))
            .bg(ui.panel())
            .border_1()
            .border_color(ui.line())
            .shadow_lg()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.menu = None;
                cx.notify();
            }))
            .child(item("vault-new", "New vault…").on_click(
                cx.listener(|this, _: &ClickEvent, _, cx| this.guard(Pending::NewVault, cx)),
            ))
            .child(item("vault-open", "Open vault…").on_click(
                cx.listener(|this, _: &ClickEvent, _, cx| this.guard(Pending::OpenVault, cx)),
            ))
            .when(open, |d| {
                d.child(
                    item("vault-scan", "Scan the network…").on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| this.open_scan_dialog(cx)),
                    ),
                )
                .child(
                    item("vault-import", "Import nmap XML…").on_click(cx.listener(
                        |this, _: &ClickEvent, _, cx| {
                            this.menu = None;
                            Self::import_nmap_dialog(cx);
                        },
                    )),
                )
                .child(
                    item("vault-close", "Close vault")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_vault(cx))),
                )
            })
    }

    /// The Web menu of the title bar: server URL and publish button.
    pub fn render_web_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.t();
        let ui = Ui { t };
        let busy = self.hub.publishing;
        div()
            .id("web-menu")
            .occlude()
            .mt(px(30.))
            .p(px(12.))
            .w(px(320.))
            .rounded(px(8.))
            .bg(ui.panel())
            .border_1()
            .border_color(ui.line())
            .shadow_lg()
            .text_color(ui.ink())
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.menu = None;
                cx.notify();
            }))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(gpui::FontWeight(600.))
                    .child("Open in the web editor"),
            )
            .child(
                div()
                    .pt(px(4.))
                    .text_size(px(11.5))
                    .text_color(ui.muted())
                    .child("Uploads this diagram to an infra-plot server and opens it in the browser, with the 3D view and SVG/PNG export."),
            )
            .child(ui.row("Server URL", self.hub.web_url.clone()))
            .child(
                div().flex().justify_end().pt(px(10.)).child(
                    ui.button("web-publish", true)
                        .child(Ui::icon("web", 14., hsla(mix(t.accent, t.ink, 0.1))))
                        .child(if busy { "Uploading…" } else { "Upload and open" })
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.publish_web(cx))),
                ),
            )
    }

    pub(super) fn toggle_menu(&mut self, menu: Menu, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = if self.menu == Some(menu) {
            None
        } else {
            Some(menu)
        };
        if self.menu == Some(Menu::Web) {
            let url = self.hub.web_url.clone();
            url.update(cx, TextInput::select_all_text);
            window.focus(&url.focus_handle(cx), cx);
        }
        cx.notify();
    }
}
