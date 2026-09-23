/** The editor shell: top bar, palette, properties panel, shortcuts and persistence. */
import { NODE_KINDS, nodeKinds, ZONE_KINDS, zoneKinds } from "../model/catalog";
import {
  detectFormat,
  emptyDoc,
  type Format,
  findElement,
  type Issue,
  parseDocument,
  serialize,
} from "../model/doc";
import { Canvas2D, isTyping } from "../render2d/canvas2d";
import { deleteElement, duplicateElement, renameId } from "../state/ops";
import { type State, Store, type Tool } from "../state/store";

const AUTOSAVE_KEY = "infraplot:autosave";

function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Partial<Record<string, string>> = {},
  ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) if (v !== undefined) e.setAttribute(k, v);
  e.append(...children);
  return e;
}

function button(label: string, onClick: () => void, title = label): HTMLButtonElement {
  const b = h("button", { type: "button", title }, label);
  b.addEventListener("click", onClick);
  return b;
}

function sameTool(a: Tool, b: Tool): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

function download(name: string, text: string, type: string): void {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const a = h("a", { href: url, download: name });
  a.click();
  URL.revokeObjectURL(url);
}

function slug(title: string): string {
  const s = title
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64);
  return s || "diagram";
}

export function mountApp(root: HTMLElement): void {
  const store = new Store();

  // ------------------------------------------------------------ toasts

  const toasts = h("div", { class: "toasts" });
  const toast = (msg: string, error = false): void => {
    const t = h("div", { class: error ? "toast error" : "toast" }, msg);
    toasts.appendChild(t);
    setTimeout(() => t.remove(), error ? 6000 : 2500);
  };
  const showIssues = (issues: Issue[]): void =>
    toast(issues.map((i) => `${i.path}: ${i.message}`).join("\n"), true);

  // ------------------------------------------------------------ io

  const importText = (name: string, text: string): void => {
    const result = parseDocument(text, detectFormat(name, text));
    if (result.ok) {
      store.load(result.doc, null);
      canvas.fit();
    } else {
      showIssues(result.issues);
    }
  };

  const exportAs = (format: Format): void => {
    const ext = format === "json" ? "json" : "toml";
    download(`${slug(store.doc.title)}.${ext}`, serialize(store.doc, format), `application/${ext}`);
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
      history.replaceState(null, "", `/d/${id}${location.search}`);
      toast(`Saved as ${id}`);
    } else if (res.status === 422) {
      const body = (await res.json()) as { issues?: Issue[] };
      showIssues(body.issues ?? []);
    } else {
      toast(`Save failed: ${res.status} ${await res.text()}`, true);
    }
  };

  const loadFromServer = async (id: string): Promise<boolean> => {
    const res = await fetch(`/api/diagrams/${id}`);
    if (!res.ok) {
      toast(`Could not load ${id}: ${res.status}`, true);
      return false;
    }
    const result = parseDocument(await res.text(), "json");
    if (!result.ok) {
      showIssues(result.issues);
      return false;
    }
    store.load(result.doc, id);
    canvas.fit();
    return true;
  };

  const fileInput = h("input", { type: "file", accept: ".json,.toml", hidden: "" });
  fileInput.addEventListener("change", async () => {
    const file = fileInput.files?.[0];
    if (file) importText(file.name, await file.text());
    fileInput.value = "";
  });

  // ------------------------------------------------------------ top bar

  const title = h("input", { class: "title", "aria-label": "Diagram title" });
  title.addEventListener("change", () => store.edit((d) => (d.title = title.value)));
  const undoBtn = button("↶", () => store.undo(), "Undo (Ctrl+Z)");
  const redoBtn = button("↷", () => store.redo(), "Redo (Ctrl+Y)");

  const topbar = h(
    "header",
    { class: "topbar" },
    title,
    undoBtn,
    redoBtn,
    h("span", { class: "spacer" }),
    button("New", () => {
      store.load(emptyDoc(), null);
      history.replaceState(null, "", "/");
    }),
    button("Open…", () => fileInput.click()),
    button("Save", () => void saveToServer(), "Save to server (Ctrl+S)"),
    button("JSON", () => exportAs("json"), "Export JSON"),
    button("TOML", () => exportAs("toml"), "Export TOML"),
    button(
      "SVG",
      () => download(`${slug(store.doc.title)}.svg`, canvas.exportSvg(""), "image/svg+xml"),
      "Export SVG",
    ),
    fileInput,
  );

  // ------------------------------------------------------------ palette

  const toolButtons: [Tool, HTMLButtonElement][] = [];
  const toolButton = (
    tool: Tool,
    label: string,
    title: string,
    color?: string,
  ): HTMLButtonElement => {
    const b = button(label, () => store.set({ tool, selection: null }), title);
    if (color) b.style.borderLeftColor = color;
    toolButtons.push([tool, b]);
    return b;
  };

  const palette = h(
    "aside",
    { class: "palette" },
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
          `Draw ${ZONE_KINDS[kind].label} (R)`,
          ZONE_KINDS[kind].color,
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
          NODE_KINDS[kind].color,
        ),
      ),
    ),
  );

  // ------------------------------------------------------------ properties

  const props = h("aside", { class: "props" });
  let propsFor: string | null = null;
  let labelInput: HTMLInputElement | HTMLTextAreaElement | null = null;

  const field = (name: string, input: HTMLElement): HTMLElement => h("label", {}, name, input);

  const renderProps = (state: State): void => {
    const found = state.selection ? findElement(state.doc, state.selection) : undefined;
    const key = found ? found.el.id : null;
    // Don't rebuild while the user is typing into the panel.
    if (key === propsFor && props.contains(document.activeElement)) return;
    propsFor = key;
    labelInput = null;
    props.replaceChildren();
    if (!found) {
      props.append(
        h("h3", {}, "Diagram"),
        h(
          "p",
          {},
          `${state.doc.nodes.length} nodes · ${state.doc.zones.length} zones · ${state.doc.edges.length} edges`,
        ),
      );
      return;
    }
    const id = found.el.id;
    props.append(h("h3", {}, found.type));

    const idInput = h("input", { value: id });
    idInput.addEventListener("change", () => {
      let ok = false;
      store.edit((d) => (ok = renameId(d, id, idInput.value.trim())));
      if (ok) store.set({ selection: idInput.value.trim() });
      else {
        toast("Id taken or invalid", true);
        idInput.value = id;
      }
    });
    props.append(field("id", idInput));

    if (found.type === "note") {
      const text = h("textarea", { rows: "4" });
      text.value = found.el.text;
      text.addEventListener("input", () =>
        store.mutate((d) => {
          const n = d.notes.find((x) => x.id === id);
          if (n) n.text = text.value;
        }),
      );
      text.addEventListener("focus", () => store.checkpoint());
      labelInput = text;
      props.append(field("text", text));
    } else if (found.type !== "line") {
      const label = h("input", { value: found.el.label ?? "" });
      label.addEventListener("focus", () => store.checkpoint());
      label.addEventListener("input", () =>
        store.mutate((d) => {
          const f = findElement(d, id);
          if (f && f.type !== "line" && f.type !== "note") f.el.label = label.value;
        }),
      );
      labelInput = label;
      props.append(field("label", label));
    }

    const color = h("input", { type: "color", value: found.el.color ?? "#1e1e1e" });
    color.addEventListener("change", () =>
      store.edit((d) => {
        const f = findElement(d, id);
        if (f) f.el.color = color.value;
      }),
    );
    props.append(field("color", color));

    props.append(
      h("p", {}),
      button("Duplicate", () => duplicate()),
      button("Delete", () => remove()),
    );
  };

  window.addEventListener("infraplot:focus-label", () => {
    renderProps(store.state);
    labelInput?.focus();
    labelInput?.select();
  });

  // ------------------------------------------------------------ stage

  const stage = h("main", { class: "stage" });
  root.append(topbar, palette, stage, props, toasts);
  const canvas = new Canvas2D(stage, store);

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

  store.subscribe((state, prev) => {
    canvas.render();
    if (document.activeElement !== title) title.value = state.doc.title;
    undoBtn.disabled = !store.canUndo;
    redoBtn.disabled = !store.canRedo;
    for (const [tool, b] of toolButtons) b.classList.toggle("active", sameTool(tool, state.tool));
    renderProps(state);
    if (state.doc !== prev.doc) localStorage.setItem(AUTOSAVE_KEY, serialize(state.doc, "json"));
    document.title = `${state.dirty ? "• " : ""}${state.doc.title} — infra-plot`;
  });

  // ------------------------------------------------------------ shortcuts

  window.addEventListener("keydown", (e) => {
    if (isTyping(e)) return;
    const mod = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();
    if (mod) {
      if (key === "z" && !e.shiftKey) store.undo();
      else if (key === "y" || (key === "z" && e.shiftKey)) store.redo();
      else if (key === "d") duplicate();
      else if (key === "s") void saveToServer();
      else return;
      e.preventDefault();
      return;
    }
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
    else if (key === "delete" || key === "backspace") remove();
    else if (key === "escape") {
      if (!canvas.cancelDrafts()) store.set({ selection: null, tool: { type: "select" } });
    } else if (key === "f") canvas.fit();
    else return;
    e.preventDefault();
  });

  // ------------------------------------------------------------ drag & drop

  window.addEventListener("dragover", (e) => e.preventDefault());
  window.addEventListener("drop", async (e) => {
    e.preventDefault();
    const file = e.dataTransfer?.files[0];
    if (file) importText(file.name, await file.text());
  });

  // ------------------------------------------------------------ boot

  void document.fonts.ready.then(() => canvas.invalidate());
  const route = /^\/d\/([^/]+)$/.exec(location.pathname);
  const saved = localStorage.getItem(AUTOSAVE_KEY);
  if (route?.[1]) {
    void loadFromServer(route[1]);
  } else if (saved) {
    const result = parseDocument(saved, "json");
    if (result.ok) store.load(result.doc, null);
  }
  store.set({});
  requestAnimationFrame(() => canvas.fit());
}
