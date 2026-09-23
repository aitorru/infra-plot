/** Standalone SVG / PNG export with the Kalam web font embedded as data URLs. */
import kalam400 from "@fontsource/kalam/files/kalam-latin-400-normal.woff2?url";
import kalam700 from "@fontsource/kalam/files/kalam-latin-700-normal.woff2?url";

const FONTS = [
  { weight: 400, url: kalam400 },
  { weight: 700, url: kalam700 },
] as const;

let fontCss: Promise<string> | null = null;

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

/** `@font-face` rules for Kalam, so exports render the same without network access. */
export function embeddedFontCss(): Promise<string> {
  fontCss ??= Promise.all(
    FONTS.map(
      async ({ weight, url }) =>
        `@font-face{font-family:Kalam;font-style:normal;font-weight:${weight};` +
        `src:url(${await dataUrl(url)}) format("woff2");}`,
    ),
  )
    .then((rules) => rules.join("\n"))
    .catch((e: unknown) => {
      fontCss = null;
      throw e;
    });
  return fontCss;
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
