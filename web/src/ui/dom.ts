/** Tiny DOM helpers shared by the editor shell. */

export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Partial<Record<string, string>> = {},
  ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) if (v !== undefined) e.setAttribute(k, v);
  e.append(...children);
  return e;
}

export function button(
  label: string | Node,
  onClick: () => void,
  title = typeof label === "string" ? label : "",
): HTMLButtonElement {
  const b = h("button", { type: "button", title }, label);
  b.addEventListener("click", onClick);
  return b;
}

export function select<T extends string>(
  value: T,
  options: readonly (readonly [T, string])[],
  onChange: (value: T) => void,
): HTMLSelectElement {
  const s = h("select", {}, ...options.map(([v, label]) => h("option", { value: v }, label)));
  s.value = value;
  s.addEventListener("change", () => onChange(s.value as T));
  return s;
}

export function downloadBlob(name: string, blob: Blob): void {
  const url = URL.createObjectURL(blob);
  const a = h("a", { href: url, download: name });
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export function download(name: string, text: string, type: string): void {
  downloadBlob(name, new Blob([text], { type }));
}

export function slug(title: string): string {
  const s = title
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64);
  return s || "diagram";
}
