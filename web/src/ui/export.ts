/** Standalone SVG / PNG export with the UI/sketch web fonts embedded as data URLs. */
import kalam400 from "@fontsource/kalam/files/kalam-latin-400-normal.woff2?url";
import kalam700 from "@fontsource/kalam/files/kalam-latin-700-normal.woff2?url";
import interVariable from "@fontsource-variable/inter/files/inter-latin-wght-normal.woff2?url";
import type { Look } from "../model/doc";

const KALAM_FONTS = [
  { weight: "400", url: kalam400 },
  { weight: "700", url: kalam700 },
] as const;

const fontCss = new Map<Look, Promise<string>>();

async function dataUrl(url: string): Promise<string> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`could not fetch ${url}: ${res.status}`);
  // Force the MIME type: static servers often send woff2 as octet-stream.
  const blob = new Blob([await res.arrayBuffer()], { type: "font/woff2" });
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(blob);
  });
}

async function kalamCss(): Promise<string> {
  const rules = await Promise.all(
    KALAM_FONTS.map(
      async ({ weight, url }) =>
        `@font-face{font-family:Kalam;font-style:normal;font-weight:${weight};` +
        `src:url(${await dataUrl(url)}) format("woff2");}`,
    ),
  );
  return rules.join("\n");
}

async function interCss(): Promise<string> {
  const url = await dataUrl(interVariable);
  return (
    "@font-face{font-family:'Inter Variable';font-style:normal;font-weight:100 900;" +
    `src:url(${url}) format("woff2-variations");}`
  );
}

/**
 * `@font-face` rules for the font the given `look` needs, so exports render the same without
 * network access: Kalam for `sketch`, Inter for `clean`.
 */
export function embeddedFontCss(look: Look): Promise<string> {
  let css = fontCss.get(look);
  if (!css) {
    css = (look === "sketch" ? kalamCss() : interCss()).catch((e: unknown) => {
      fontCss.delete(look);
      throw e;
    });
    fontCss.set(look, css);
  }
  return css;
}

/** Rasterises a standalone SVG string (as produced by `Canvas2D.exportSvg`). */
export async function svgToPng(svg: string, scale = 2): Promise<Blob> {
  const doc = new DOMParser().parseFromString(svg, "image/svg+xml").documentElement;
  const width = Number(doc.getAttribute("width") ?? 800);
  const height = Number(doc.getAttribute("height") ?? 600);
  const url = URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" }));
  try {
    const img = new Image();
    img.src = url;
    await img.decode();
    const canvas = document.createElement("canvas");
    canvas.width = Math.ceil(width * scale);
    canvas.height = Math.ceil(height * scale);
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("2D canvas not available");
    ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
    return await new Promise<Blob>((resolve, reject) =>
      canvas.toBlob(
        (b) => (b ? resolve(b) : reject(new Error("PNG encoding failed"))),
        "image/png",
      ),
    );
  } finally {
    URL.revokeObjectURL(url);
  }
}
