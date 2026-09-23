/** The editor shell: top bar, palette, properties panel, shortcuts, routing and persistence. */
import { NODE_KINDS, nodeKinds, ZONE_KINDS, zoneKinds } from "../model/catalog";
import {
  type Doc,
  detectFormat,
  emptyDoc,
  type Format,
  type Issue,
  parseDocument,
  serialize,
} from "../model/doc";
import { Canvas2D, isTyping } from "../render2d/canvas2d";
import { Scene3D } from "../render3d/scene3d";
import { deleteElement, duplicateElement } from "../state/ops";
import { Store, type Tool, type View } from "../state/store";
import { button, download, downloadBlob, h, slug } from "./dom";
import { embeddedFontCss, svgToPng } from "./export";
import { nodeIcon, zoneIcon } from "./icons";
import { type Example, Library } from "./library";
import { PropsPanel } from "./props";
import { parseRoute, updateRoute } from "./route";

const AUTOSAVE_KEY = "infraplot:autosave";

function sameTool(a: Tool, b: Tool): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

export function mountApp(root: HTMLElement): void {
  const route = parseRoute();
  const store = new Store({
    view: route.view,
    animate: !matchMedia("(prefers-reduced-motion: reduce)").matches,
  });
  root.classList.toggle("embed", route.embed);

  // ------------------------------------------------------------ toasts

  const toasts = h("div", { class: "toasts", role: "status" });
  const toast = (msg: string, error = false): void => {
    const t = h("div", { class: error ? "toast error" : "toast" }, msg);
    toasts.appendChild(t);
    setTimeout(() => t.remove(), error ? 6000 : 2500);
  };
  const showIssues = (issues: Issue[]): void =>
    toast(issues.map((i) => `${i.path}: ${i.message}`).join("\n"), true);

  // ------------------------------------------------------------ io

  /** Loads a parsed document; returns false (after reporting) if it is invalid. */
  const openText = (name: string, text: string, docId: string | null = null): boolean => {
    const result = parseDocument(text, detectFormat(name, text));
    if (!result.ok) {
      showIssues(result.issues);
      return false;
    }
    openDoc(result.doc, docId);
    return true;
  };

  const openDoc = (doc: Doc, docId: string | null): void => {
    store.load(doc, docId);
    updateRoute({ docId, src: null });
    requestAnimationFrame(fit);
  };

  const openExample = async (ex: Example): Promise<void> => {
    openText(ex.name, await ex.load());
  };

  const loadFromServer = async (id: string): Promise<boolean> => {
    const res = await fetch(`/api/diagrams/${encodeURIComponent(id)}`);
    if (!res.ok) {
      toast(`Could not load ${id}: ${res.status}`, true);
      return false;
    }
    return openText(`${id}.json`, await res.text(), id);
  };

  const loadFromUrl = async (src: string): Promise<boolean> => {
    let res: Response;
    try {
      res = await fetch(src);
    } catch (e) {
      toast(`Could not fetch ${src}: ${e}`, true);
      return false;
    }
    if (!res.ok) {
      toast(`Could not fetch ${src}: ${res.status}`, true);
      return false;
    }
    const text = await res.text();
    const result = parseDocument(text, detectFormat(new URL(src, location.href).pathname, text));
    if (!result.ok) {
      showIssues(result.issues);
      return false;
    }
    // Keep `?src=` so the link stays shareable until the user saves elsewhere.
    store.load(result.doc, null);
    requestAnimationFrame(fit);
    return true;
  };

  const exportAs = (format: Format): void => {
    const ext = format === "json" ? "json" : "toml";
    download(`${slug(store.doc.title)}.${ext}`, serialize(store.doc, format), `application/${ext}`);
  };

  const standaloneSvg = async (): Promise<string> => {
    let css = "";
    try {
      css = await embeddedFontCss();
    } catch (e) {
      toast(`Font not embedded: ${e}`, true);
    }
    return canvas.exportSvg(css);
  };

  const exportSvg = async (): Promise<void> => {
    download(`${slug(store.doc.title)}.svg`, await standaloneSvg(), "image/svg+xml");
  };

  const exportPng = async (): Promise<void> => {
    try {
      const png =
        store.state.view === "3d"
          ? await scene3d.exportPng(3)
          : await svgToPng(await standaloneSvg(), 2);
      downloadBlob(`${slug(store.doc.title)}${store.state.view === "3d" ? "-3d" : ""}.png`, png);
    } catch (e) {
      toast(`PNG export failed: ${e}`, true);
    }
  };

  const saveToServer = async (): Promise<void> => {
    const id = store.state.docId ?? slug(store.doc.title);
    const res = await fetch(`/api/diagrams/${id}`, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: serialize(store.doc, "json"),
    });
    if (res.ok) {
      store.set({ docId: id, dirty: false });
      updateRoute({ docId: id, src: null });
      toast(`Saved as ${id}`);
    } else if (res.status === 422) {
      const body = (await res.json()) as { issues?: Issue[] };
      showIssues(body.issues ?? []);
    } else {
      toast(`Save failed: ${res.status} ${await res.text()}`, true);
    }
  };

  const fileInput = h("input", {
    type: "file",
    accept: ".json,.toml,application/json,application/toml",
    hidden: "",
    "data-testid": "file-input",
  });
  fileInput.addEventListener("change", async () => {
    const file = fileInput.files?.[0];
    if (file) openText(file.name, await file.text());
    fileInput.value = "";
  });

  const library = new Library({
    openServer: (id) => void loadFromServer(id),
    openExample: (ex) => void openExample(ex),
    openFile: () => fileInput.click(),
    toast,
  });

  // ------------------------------------------------------------ top bar

  const title = h("input", { class: "title", "aria-label": "Diagram title" });
  title.addEventListener("change", () => store.edit((d) => (d.title = title.value)));
  const undoBtn = button("↶", () => store.undo(), "Undo (Ctrl+Z)");
  const redoBtn = button("↷", () => store.redo(), "Redo (Ctrl+Y)");

  const setView = (view: View): void => {
    store.set({ view });
    updateRoute({ view });
  };
  const viewButtons: [View, HTMLButtonElement][] = (["2d", "3d"] as const).map((v) => {
    const b = button(v.toUpperCase(), () => setView(v), `${v.toUpperCase()} view (${v[0]})`);
    b.dataset.view = v;
    return [v, b];
  });

  const topbar = h(
    "header",
    { class: "topbar" },
    title,
    h("span", { class: "group" }, undoBtn, redoBtn),
    h(
      "span",
      { class: "group segmented", role: "group", "aria-label": "View" },
      ...viewButtons.map(([, b]) => b),
    ),
    h("span", { class: "spacer" }),
    h(
      "span",
      { class: "group" },
      button(
        "New",
        () => {
          openDoc(emptyDoc(), null);
        },
        "New diagram",
      ),
      button("Open…", () => void library.show(), "Saved diagrams and examples (Ctrl+O)"),
      button("Import…", () => fileInput.click(), "Import a .json / .toml file"),
      button("Save", () => void saveToServer(), "Save to server (Ctrl+S)"),
    ),
    h(
      "span",
      { class: "group", role: "group", "aria-label": "Export" },
      h("span", { class: "muted" }, "Export"),
      button("JSON", () => exportAs("json"), "Export JSON"),
      button("TOML", () => exportAs("toml"), "Export TOML"),
      button("SVG", () => void exportSvg(), "Export SVG"),
      button("PNG", () => void exportPng(), "Export PNG (2× in 2D, 3× photo of the 3D view)"),
    ),
    fileInput,
  );

  // ------------------------------------------------------------ palette

  const toolButtons: [Tool, HTMLButtonElement][] = [];
  const toolButton = (tool: Tool, label: string, title: string, icon?: SVGSVGElement) => {
    const content = icon ? h("span", { class: "tile" }, icon, h("span", {}, label)) : label;
    const b = button(content, () => store.set({ tool, selection: null }), title);
    b.dataset.tool = tool.type === "node" || tool.type === "zone" ? tool.kind : tool.type;
    toolButtons.push([tool, b]);
    return b;
  };

  const palette = h(
    "aside",
    { class: "palette", "data-testid": "palette" },
    h("h3", {}, "Tools"),
    h(
      "div",
      { class: "tools" },
      toolButton({ type: "select" }, "Select", "Select (V)"),
      toolButton({ type: "hand" }, "Hand", "Pan (H)"),
      toolButton({ type: "connect" }, "Connect", "Connect nodes (C)"),
      toolButton({ type: "line" }, "Line", "Free line (L)"),
      toolButton({ type: "note" }, "Note", "Text note (T)"),
    ),
    h("h3", {}, "Zones"),
    h(
      "div",
      { class: "grid" },
      ...zoneKinds().map((kind) =>
        toolButton(
          { type: "zone", kind },
          ZONE_KINDS[kind].label,
          `Draw ${ZONE_KINDS[kind].label}${kind === "generic" ? " (R)" : ""}`,
          zoneIcon(kind),
        ),
      ),
    ),
    h("h3", {}, "Nodes"),
    h(
      "div",
      { class: "grid" },
      ...nodeKinds().map((kind) =>
        toolButton(
          { type: "node", kind },
          NODE_KINDS[kind].label,
          `Place ${NODE_KINDS[kind].label}`,
          nodeIcon(kind),
        ),
      ),
    ),
  );

  // ------------------------------------------------------------ properties

  const remove = (): void => {
    const id = store.state.selection;
    if (!id) return;
    store.edit((d) => deleteElement(d, id));
    store.set({ selection: null });
  };

  const duplicate = (): void => {
    const id = store.state.selection;
    if (!id) return;
    let copy: string | undefined;
    store.edit((d) => (copy = duplicateElement(d, id)));
    if (copy) store.set({ selection: copy });
  };

  const props = new PropsPanel(store, { toast, duplicate, remove });
  window.addEventListener("infraplot:focus-label", () => props.focusLabel());

  // ------------------------------------------------------------ stage

  const stage = h("main", { class: "stage" });
  const view3d = h("div", { class: "view3d", "data-testid": "view-3d", hidden: "" });
  root.append(topbar, palette, stage, props.el, toasts, library.el);
  const canvas = new Canvas2D(stage, store);
  stage.append(view3d);
  const scene3d = new Scene3D(view3d, store);
  const fit = (): void => {
    canvas.fit();
    scene3d.fit();
  };

  const toggleAnimate = (): void => store.set({ animate: !store.state.animate });
  const packetsBtn = button("Packets", toggleAnimate, "Animate packets along edges (P)");
  if (scene3d.available) {
    view3d.append(
      h(
        "div",
        { class: "toolbar3d", "data-testid": "toolbar-3d" },
        button("⟲", () => scene3d.rotate(-1), "Rotate 90° left (Q)"),
        button("⟳", () => scene3d.rotate(1), "Rotate 90° right (E)"),
        button("Fit", () => scene3d.fit(), "Fit the diagram (F)"),
        packetsBtn,
        h(
          "span",
          { class: "muted hint" },
          "Drag: pan · right-drag: orbit · wheel: zoom · zones, lines and notes are drawn in 2D",
        ),
      ),
    );
  }

  store.subscribe((state, prev) => {
    canvas.render();
    scene3d.render();
    if (document.activeElement !== title) title.value = state.doc.title;
    undoBtn.disabled = !store.canUndo;
    redoBtn.disabled = !store.canRedo;
    for (const [tool, b] of toolButtons) b.classList.toggle("active", sameTool(tool, state.tool));
    for (const [view, b] of viewButtons) b.classList.toggle("active", view === state.view);
    canvas.svg.toggleAttribute("hidden", state.view !== "2d");
    view3d.toggleAttribute("hidden", state.view !== "3d");
    scene3d.setVisible(state.view === "3d");
    packetsBtn.classList.toggle("active", state.animate);
    packetsBtn.setAttribute("aria-pressed", String(state.animate));
    root.dataset.view = state.view;
    if (state.view === "2d" && prev.view !== "2d") requestAnimationFrame(() => canvas.fit());
    props.render(state);
    if (state.doc !== prev.doc && !route.embed) {
      localStorage.setItem(AUTOSAVE_KEY, serialize(state.doc, "json"));
    }
    document.title = `${state.dirty ? "• " : ""}${state.doc.title} — infra-plot`;
  });

  // ------------------------------------------------------------ shortcuts

  window.addEventListener("keydown", (e) => {
    if (isTyping(e) || library.el.open) return;
    const mod = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();
    if (mod) {
      if (key === "z" && !e.shiftKey) store.undo();
      else if (key === "y" || (key === "z" && e.shiftKey)) store.redo();
      else if (key === "d") duplicate();
      else if (key === "s") void saveToServer();
      else if (key === "o") void library.show();
      else return;
      e.preventDefault();
      return;
    }
    if (e.altKey) return;
    const tools: Record<string, Tool> = {
      v: { type: "select" },
      h: { type: "hand" },
      r: { type: "zone", kind: "generic" },
      c: { type: "connect" },
      l: { type: "line" },
      t: { type: "note" },
    };
    const tool = tools[key];
    if (tool) store.set({ tool });
    else if (key === "2") setView("2d");
    else if (key === "3") setView("3d");
    else if (key === "p") toggleAnimate();
    else if ((key === "q" || key === "e") && store.state.view === "3d")
      scene3d.rotate(key === "q" ? -1 : 1);
    else if (key === "delete" || key === "backspace") remove();
    else if (key === "escape") {
      if (!canvas.cancelDrafts()) store.set({ selection: null, tool: { type: "select" } });
    } else if (key === "f") {
      if (store.state.view === "3d") scene3d.fit();
      else canvas.fit();
    } else return;
    e.preventDefault();
  });

  // ------------------------------------------------------------ drag & drop

  window.addEventListener("dragover", (e) => {
    e.preventDefault();
    root.classList.add("dropping");
  });
  window.addEventListener("dragleave", (e) => {
    if (!e.relatedTarget) root.classList.remove("dropping");
  });
  window.addEventListener("drop", async (e) => {
    e.preventDefault();
    root.classList.remove("dropping");
    const file = e.dataTransfer?.files[0];
    if (file) openText(file.name, await file.text());
  });

  // ------------------------------------------------------------ boot

  const boot = async (): Promise<void> => {
    if (route.src) {
      await loadFromUrl(route.src);
    } else if (route.docId) {
      await loadFromServer(route.docId);
    } else {
      const saved = route.embed ? null : localStorage.getItem(AUTOSAVE_KEY);
      const result = saved ? parseDocument(saved, "json") : null;
      if (result?.ok) store.load(result.doc, null);
    }
    store.set({});
    // Load Kalam explicitly: the 3D labels are canvas textures, which don't request fonts.
    await Promise.all([document.fonts.load("20px Kalam"), document.fonts.load("700 20px Kalam")]);
    await document.fonts.ready;
    canvas.invalidate();
    scene3d.invalidate();
    fit();
    scene3d.flush();
    // Lets screenshot tooling wait for a fully drawn diagram.
    root.dataset.ready = "true";
  };
  void boot();
}
