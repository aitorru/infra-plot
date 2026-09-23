import type { Doc, NodeKind, ZoneKind } from "../model/doc";
import { emptyDoc } from "../model/doc";

export type Tool =
  | { type: "select" }
  | { type: "hand" }
  | { type: "zone"; kind: ZoneKind }
  | { type: "connect" }
  | { type: "line" }
  | { type: "note" }
  | { type: "node"; kind: NodeKind };

export type View = "2d" | "3d";

export interface State {
  doc: Doc;
  selection: string | null;
  tool: Tool;
  view: View;
  /** Server id the doc was loaded from / saved to. */
  docId: string | null;
  dirty: boolean;
  animate: boolean;
}

type Listener = (state: State, prev: State) => void;

const HISTORY_LIMIT = 200;

export class Store {
  #state: State;
  #listeners = new Set<Listener>();
  #undo: Doc[] = [];
  #redo: Doc[] = [];
  #pending: State | null = null;

  constructor(init?: Partial<State>) {
    this.#state = {
      doc: emptyDoc(),
      selection: null,
      tool: { type: "select" },
      view: "2d",
      docId: null,
      dirty: false,
      animate: true,
      ...init,
    };
  }

  get state(): State {
    return this.#state;
  }

  get doc(): Doc {
    return this.#state.doc;
  }

  subscribe(fn: Listener): () => void {
    this.#listeners.add(fn);
    return () => this.#listeners.delete(fn);
  }

  set(patch: Partial<State>): void {
    const prev = this.#pending ?? this.#state;
    this.#state = { ...this.#state, ...patch };
    this.#schedule(prev);
  }

  /** Records the current doc so the next mutation can be undone. */
  checkpoint(): void {
    this.#undo.push(structuredClone(this.#state.doc));
    if (this.#undo.length > HISTORY_LIMIT) this.#undo.shift();
    this.#redo = [];
  }

  /** Mutates the doc in place without touching history (e.g. mid-drag). */
  mutate(fn: (doc: Doc) => void): void {
    const doc = structuredClone(this.#state.doc);
    fn(doc);
    this.set({ doc, dirty: true });
  }

  /** A single undoable change. */
  edit(fn: (doc: Doc) => void): void {
    this.checkpoint();
    this.mutate(fn);
  }

  /** Replaces the whole document (open / import / new). */
  load(doc: Doc, docId: string | null): void {
    this.#undo = [];
    this.#redo = [];
    this.set({ doc, docId, selection: null, dirty: false, tool: { type: "select" } });
  }

  get canUndo(): boolean {
    return this.#undo.length > 0;
  }

  get canRedo(): boolean {
    return this.#redo.length > 0;
  }

  undo(): void {
    const doc = this.#undo.pop();
    if (!doc) return;
    this.#redo.push(this.#state.doc);
    this.#restore(doc);
  }

  redo(): void {
    const doc = this.#redo.pop();
    if (!doc) return;
    this.#undo.push(this.#state.doc);
    this.#restore(doc);
  }

  #restore(doc: Doc): void {
    const ids = new Set(
      [doc.zones, doc.nodes, doc.edges, doc.lines, doc.notes].flat().map((e) => e.id),
    );
    const selection =
      this.#state.selection && ids.has(this.#state.selection) ? this.#state.selection : null;
    this.set({ doc, selection, dirty: true });
  }

  /** Listeners run once per microtask, however many updates happened. */
  #schedule(prev: State): void {
    if (this.#pending) return;
    this.#pending = prev;
    queueMicrotask(() => {
      const before = this.#pending ?? prev;
      this.#pending = null;
      for (const fn of this.#listeners) fn(this.#state, before);
    });
  }
}
