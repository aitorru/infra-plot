/** The properties panel: per-type fields for the selected element, or the diagram itself. */
import { INK, NODE_KINDS, nodeKinds, ZONE_KINDS, zoneKinds } from "../model/catalog";
import type {
  Arrow,
  Doc,
  Element,
  NodeKind,
  Route,
  StrokeStyle,
  TextSize,
  ZoneKind,
} from "../model/doc";
import { findElement } from "../model/doc";
import { renameId } from "../state/ops";
import type { State, Store } from "../state/store";
import { button, h, select } from "./dom";

export interface PropsActions {
  toast(msg: string, error?: boolean): void;
  duplicate(): void;
  remove(): void;
}

const STYLES: readonly (readonly [StrokeStyle, string])[] = [
  ["solid", "Solid"],
  ["dashed", "Dashed"],
  ["dotted", "Dotted"],
];
const ARROWS: readonly (readonly [Arrow, string])[] = [
  ["none", "None"],
  ["end", "→ End"],
  ["start", "← Start"],
  ["both", "↔ Both"],
];
const ROUTES: readonly (readonly [Route, string])[] = [
  ["straight", "Straight"],
  ["orthogonal", "Orthogonal"],
];
const SIZES: readonly (readonly [TextSize, string])[] = [
  ["s", "Small"],
  ["m", "Medium"],
  ["l", "Large"],
  ["xl", "Extra large"],
];

function defaultColor(found: Element): string {
  if (found.type === "node") return NODE_KINDS[found.el.kind].color;
  if (found.type === "zone") return ZONE_KINDS[found.el.kind ?? "generic"].color;
  return INK;
}

function field(name: string, ...inputs: HTMLElement[]): HTMLElement {
  return h("label", {}, h("span", {}, name), ...inputs);
}

/** Text-like inputs keep the panel from being rebuilt under the user's cursor. */
function isEditingText(el: globalThis.Element | null): boolean {
  return (
    el instanceof HTMLTextAreaElement ||
    (el instanceof HTMLInputElement && el.type !== "color" && el.type !== "checkbox")
  );
}

export class PropsPanel {
  readonly el = h("aside", { class: "props", "data-testid": "props" });
  #store: Store;
  #actions: PropsActions;
  #for: string | null = null;
  #label: HTMLInputElement | HTMLTextAreaElement | null = null;

  constructor(store: Store, actions: PropsActions) {
    this.#store = store;
    this.#actions = actions;
  }

  focusLabel(): void {
    this.render(this.#store.state, true);
    this.#label?.focus();
    this.#label?.select();
  }

  render(state: State, force = false): void {
    const found = state.selection ? findElement(state.doc, state.selection) : undefined;
    const key = found ? found.el.id : null;
    if (
      !force &&
      key === this.#for &&
      this.el.contains(document.activeElement) &&
      isEditingText(document.activeElement)
    )
      return;
    this.#for = key;
    this.#label = null;
    this.el.replaceChildren();
    delete this.el.dataset.type;
    if (found) this.#element(found);
    else this.#diagram(state.doc);
  }

  // ------------------------------------------------------------ helpers

  /** An undoable edit of the element with `id` (re-found in the cloned doc). */
  #edit(id: string, fn: (found: Element) => void): void {
    this.#store.edit((d) => {
      const f = findElement(d, id);
      if (f) fn(f);
    });
  }

  /** Live edits while typing: one checkpoint on focus, then untracked mutations. */
  #live<T extends HTMLInputElement | HTMLTextAreaElement>(
    input: T,
    id: string,
    fn: (found: Element, value: string) => void,
  ): T {
    input.addEventListener("focus", () => this.#store.checkpoint());
    input.addEventListener("input", () =>
      this.#store.mutate((d) => {
        const f = findElement(d, id);
        if (f) fn(f, input.value);
      }),
    );
    return input;
  }

  // ------------------------------------------------------------ diagram

  #diagram(doc: Doc): void {
    const desc = h("textarea", { rows: "4", placeholder: "What this diagram shows…" });
    desc.value = doc.description;
    desc.addEventListener("focus", () => this.#store.checkpoint());
    desc.addEventListener("input", () => this.#store.mutate((d) => (d.description = desc.value)));
    this.el.append(
      h("h3", {}, "Diagram"),
      h(
        "p",
        { class: "stats" },
        `${doc.nodes.length} nodes · ${doc.zones.length} zones · ${doc.edges.length} edges · ` +
          `${doc.lines.length} lines · ${doc.notes.length} notes`,
      ),
      field("description", desc),
    );
  }

  // ------------------------------------------------------------ element

  #element(found: Element): void {
    const id = found.el.id;
    this.el.append(h("h3", {}, found.type));
    this.el.dataset.type = found.type;

    const idInput = h("input", { value: id, name: "id", spellcheck: "false" });
    idInput.addEventListener("change", () => {
      const next = idInput.value.trim();
      let ok = false;
      this.#store.edit((d) => (ok = renameId(d, id, next)));
      if (ok) this.#store.set({ selection: next });
      else {
        this.#actions.toast("Id taken or invalid", true);
        idInput.value = id;
      }
    });
    this.el.append(field("id", idInput));

    switch (found.type) {
      case "node":
        this.#labelField(id, found.el.label);
        this.el.append(
          field(
            "kind",
            select<NodeKind>(
              found.el.kind,
              nodeKinds().map((k) => [k, NODE_KINDS[k].label] as const),
              (kind) =>
                this.#edit(id, (f) => {
                  if (f.type === "node") f.el.kind = kind;
                }),
            ),
          ),
        );
        break;
      case "zone": {
        this.#labelField(id, found.el.label);
        const kind = found.el.kind ?? "generic";
        this.el.append(
          field(
            "kind",
            select<ZoneKind>(
              kind,
              zoneKinds().map((k) => [k, ZONE_KINDS[k].label] as const),
              (k) =>
                this.#edit(id, (f) => {
                  if (f.type === "zone") f.el.kind = k;
                }),
            ),
          ),
          field(
            "style",
            select<StrokeStyle | "">(
              found.el.style ?? "",
              [["", `Default (${ZONE_KINDS[kind].style})`], ...STYLES],
              (style) =>
                this.#edit(id, (f) => {
                  if (f.type !== "zone") return;
                  if (style) f.el.style = style;
                  else delete f.el.style;
                }),
            ),
          ),
        );
        break;
      }
      case "edge": {
        this.#labelField(id, found.el.label);
        this.el.append(
          h("p", { class: "stats" }, `${found.el.from} → ${found.el.to}`),
          this.#styleField(id, found.el.style),
          this.#arrowField(id, found.el.arrow ?? "end"),
          field(
            "route",
            select<Route>(found.el.route ?? "straight", ROUTES, (route) =>
              this.#edit(id, (f) => {
                if (f.type === "edge") f.el.route = route;
              }),
            ),
          ),
        );
        break;
      }
      case "line":
        this.el.append(
          this.#styleField(id, found.el.style),
          this.#arrowField(id, found.el.arrow ?? "none"),
        );
        break;
      case "note": {
        const text = h("textarea", { rows: "4", name: "text" });
        text.value = found.el.text;
        this.#label = this.#live(text, id, (f, v) => {
          if (f.type === "note") f.el.text = v;
        });
        this.el.append(
          field("text", text),
          field(
            "size",
            select<TextSize>(found.el.size ?? "m", SIZES, (size) =>
              this.#edit(id, (f) => {
                if (f.type === "note") f.el.size = size;
              }),
            ),
          ),
        );
        break;
      }
    }

    this.#colorField(found);
    if (found.type === "node") this.#metaField(id, found.el.meta ?? {});

    const actions = h("div", { class: "actions" });
    if (found.type !== "edge") actions.append(button("Duplicate", () => this.#actions.duplicate()));
    actions.append(button("Delete", () => this.#actions.remove()));
    this.el.append(actions);
  }

  #labelField(id: string, value: string | undefined): void {
    const input = h("input", { value: value ?? "", name: "label" });
    this.#label = this.#live(input, id, (f, v) => {
      if (f.type !== "line" && f.type !== "note") f.el.label = v;
    });
    this.el.append(field("label", input));
  }

  #styleField(id: string, value: StrokeStyle | undefined): HTMLElement {
    return field(
      "style",
      select<StrokeStyle>(value ?? "solid", STYLES, (style) =>
        this.#edit(id, (f) => {
          if (f.type === "edge" || f.type === "line") f.el.style = style;
        }),
      ),
    );
  }

  #arrowField(id: string, value: Arrow): HTMLElement {
    return field(
      "arrow",
      select<Arrow>(value, ARROWS, (arrow) =>
        this.#edit(id, (f) => {
          if (f.type === "edge" || f.type === "line") f.el.arrow = arrow;
        }),
      ),
    );
  }

  #colorField(found: Element): void {
    const id = found.el.id;
    const color = h("input", {
      type: "color",
      name: "color",
      value: found.el.color ?? defaultColor(found),
    });
    color.addEventListener("change", () =>
      this.#edit(id, (f) => {
        f.el.color = color.value;
      }),
    );
    const reset = button(
      "↺",
      () =>
        this.#edit(id, (f) => {
          delete f.el.color;
        }),
      "Reset to the default colour",
    );
    reset.disabled = found.el.color == null;
    this.el.append(field("color", h("span", { class: "row" }, color, reset)));
  }

  #metaField(id: string, meta: Record<string, string>): void {
    const rows = h("div", { class: "meta" });
    const commit = (): void => {
      const next: Record<string, string> = {};
      for (const row of rows.querySelectorAll<HTMLElement>(".meta-row")) {
        const [k, v] = row.querySelectorAll("input");
        const key = k?.value.trim();
        if (key) next[key] = v?.value ?? "";
      }
      this.#edit(id, (f) => {
        if (f.type !== "node") return;
        if (Object.keys(next).length > 0) f.el.meta = next;
        else delete f.el.meta;
      });
    };
    const addRow = (key: string, value: string): HTMLInputElement => {
      const k = h("input", { value: key, placeholder: "key", "aria-label": "meta key" });
      const v = h("input", { value, placeholder: "value", "aria-label": "meta value" });
      k.addEventListener("change", commit);
      v.addEventListener("change", commit);
      const row = h("div", { class: "meta-row" }, k, v);
      row.append(
        button(
          "×",
          () => {
            row.remove();
            commit();
          },
          "Remove entry",
        ),
      );
      rows.append(row);
      return k;
    };
    for (const [k, v] of Object.entries(meta)) addRow(k, v);
    this.el.append(
      h("h3", {}, "meta"),
      rows,
      button("+ Add", () => addRow("", "").focus(), "Add a key/value entry"),
    );
  }
}
