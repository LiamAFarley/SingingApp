/** Colour helpers for answer buttons tinted with a label's colour. */

type Rgb = [number, number, number];

const DARK_TEXT = "#1f2328";
const LIGHT_TEXT = "#ffffff";

/** `#rgb` or `#rrggbb`; anything else is null. */
function parseHex(c: string): Rgb | null {
  const m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(c.trim());
  if (!m) return null;
  const h = m[1].length === 3 ? [...m[1]].map((x) => x + x).join("") : m[1];
  return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)) as Rgb;
}

/** `share` of `a` with the rest `b`. */
function mix(a: Rgb, b: Rgb, share: number): Rgb {
  return a.map((v, i) => Math.round(v * share + b[i] * (1 - share))) as Rgb;
}

/** WCAG relative luminance. */
function luminance([r, g, b]: Rgb): number {
  const lin = (v: number) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

function contrast(a: number, b: number): number {
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

/**
 * Background `share` of `color` over `base`, with whichever of white or near-black text reads
 * better on it. Undefined when the colour is not a hex colour.
 */
export function tint(color: string, base: string, share: number): { background: string; color: string } | undefined {
  const c = parseHex(color);
  const b = parseHex(base);
  if (!c || !b) return undefined;
  const bg = mix(c, b, share);
  const l = luminance(bg);
  const text = contrast(l, 1) >= contrast(l, luminance(parseHex(DARK_TEXT)!)) ? LIGHT_TEXT : DARK_TEXT;
  return { background: `rgb(${bg.join(", ")})`, color: text };
}
