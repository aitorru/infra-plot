/** The "Open" dialog: diagrams saved on the server plus the bundled examples. */
import { button, h } from "./dom";

export interface Summary {
  id: string;
  title: string;
  /** Seconds since the Unix epoch. */
  updated_at: number;
}

const exampleFiles = import.meta.glob<string>("../../../examples/*.{json,toml}", {
  query: "?raw",
  import: "default",
});

export interface Example {
  name: string;
  load: () => Promise<string>;
}

export const EXAMPLES: readonly Example[] = Object.entries(exampleFiles)
  .map(([path, load]) => ({ name: path.slice(path.lastIndexOf("/") + 1), load }))
  .sort((a, b) => a.name.localeCompare(b.name));

export async function listDiagrams(): Promise<Summary[]> {
  const res = await fetch("/api/diagrams");
  if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
  return (await res.json()) as Summary[];
}

export interface LibraryActions {
  openServer(id: string): void;
  openExample(example: Example): void;
  openFile(): void;
  toast(msg: string, error?: boolean): void;
}

const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });

export class Library {
  readonly el = h("dialog", { class: "library", "data-testid": "library" });
  #server = h("ul", { class: "list", "data-testid": "server-diagrams" });
  #actions: LibraryActions;

  constructor(actions: LibraryActions) {
    this.#actions = actions;
    const examples = h(
      "ul",
      { class: "list", "data-testid": "examples" },
      ...EXAMPLES.map((ex) =>
        h(
          "li",
          {},
          button(ex.name, () => {
            this.el.close();
            actions.openExample(ex);
          }),
        ),
      ),
    );
    this.el.append(
      h(
        "header",
        {},
        h("h2", {}, "Open diagram"),
        button("×", () => this.el.close(), "Close"),
      ),
      h("h3", {}, "On the server"),
      this.#server,
      h("h3", {}, "Examples"),
      examples,
      h(
        "p",
        {},
        button("From file…", () => {
          this.el.close();
          actions.openFile();
        }),
        " or drop a .json / .toml file anywhere.",
      ),
    );
    // Clicking the backdrop closes the dialog.
    this.el.addEventListener("click", (e) => {
      if (e.target === this.el) this.el.close();
    });
  }

  async show(): Promise<void> {
    this.el.showModal();
    await this.refresh();
  }

  async refresh(): Promise<void> {
    this.#server.replaceChildren(h("li", { class: "muted" }, "Loading…"));
    let items: Summary[];
    try {
      items = await listDiagrams();
    } catch (e) {
      this.#server.replaceChildren(h("li", { class: "muted" }, `Server unavailable: ${e}`));
      return;
    }
    if (items.length === 0) {
      this.#server.replaceChildren(h("li", { class: "muted" }, "Nothing saved yet (Ctrl+S)."));
      return;
    }
    this.#server.replaceChildren(
      ...items.map((d) => {
        const open = button(
          h(
            "span",
            {},
            h("strong", {}, d.title),
            h("small", {}, ` ${d.id} · ${dateFormat.format(new Date(d.updated_at * 1000))}`),
          ),
          () => {
            this.el.close();
            this.#actions.openServer(d.id);
          },
          `Open ${d.id}`,
        );
        const del = button("🗑", () => void this.#delete(d), `Delete ${d.id}`);
        del.classList.add("danger");
        del.setAttribute("aria-label", `Delete ${d.id}`);
        return h("li", { "data-id": d.id }, open, del);
      }),
    );
  }

  async #delete(d: Summary): Promise<void> {
    if (!confirm(`Delete "${d.title}" (${d.id}) from the server?`)) return;
    const res = await fetch(`/api/diagrams/${d.id}`, { method: "DELETE" });
    if (res.ok) this.#actions.toast(`Deleted ${d.id}`);
    else this.#actions.toast(`Delete failed: ${res.status}`, true);
    await this.refresh();
  }
}
