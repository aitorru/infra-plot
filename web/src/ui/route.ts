/** URL state: `/d/:id`, `?view=3d`, `?embed=1`, `?src=<url>`. */
import type { View } from "../state/store";

export interface Route {
  /** Server id from `/d/:id`. */
  docId: string | null;
  view: View;
  /** Canvas only, no editor chrome (for screenshots and iframes). */
  embed: boolean;
  /** Load the diagram from this URL (JSON or TOML) instead of the server. */
  src: string | null;
}

export function parseRoute(loc: Location = location): Route {
  const params = new URLSearchParams(loc.search);
  const embed = params.get("embed");
  return {
    docId: /^\/d\/([^/]+)\/?$/.exec(loc.pathname)?.[1] ?? null,
    view: params.get("view") === "3d" ? "3d" : "2d",
    embed: embed !== null && embed !== "0" && embed !== "false",
    src: params.get("src"),
  };
}

/** Rewrites the address bar without reloading; `null` removes a parameter. */
export function updateRoute(patch: {
  docId?: string | null;
  view?: View;
  src?: string | null;
}): void {
  const url = new URL(location.href);
  if (patch.docId !== undefined) url.pathname = patch.docId ? `/d/${patch.docId}` : "/";
  if (patch.view !== undefined) {
    if (patch.view === "3d") url.searchParams.set("view", "3d");
    else url.searchParams.delete("view");
  }
  if (patch.src !== undefined) {
    if (patch.src) url.searchParams.set("src", patch.src);
    else url.searchParams.delete("src");
  }
  if (url.href !== location.href) history.replaceState(null, "", url);
}
