// oklch -> sRGB dla canvas (wszystkie serie wykresu maja te sama jasnosc i chrome, rozni je odcien).

export function oklch(l: number, c: number, h: number, alfa = 1): string {
  const hr = (h * Math.PI) / 180;
  const a = c * Math.cos(hr);
  const b = c * Math.sin(hr);
  const l_ = l + 0.3963377774 * a + 0.2158037573 * b;
  const m_ = l - 0.1055613458 * a - 0.0638541728 * b;
  const s_ = l - 0.0894841775 * a - 1.291485548 * b;
  const L3 = l_ ** 3;
  const M3 = m_ ** 3;
  const S3 = s_ ** 3;
  const lin = [
    4.0767416621 * L3 - 3.3077115913 * M3 + 0.2309699292 * S3,
    -1.2684380046 * L3 + 2.6097574011 * M3 - 0.3413193965 * S3,
    -0.0041960863 * L3 - 0.7034186147 * M3 + 1.707614701 * S3,
  ];
  const [r, g, bl] = lin.map((x) => {
    const v = Math.max(0, Math.min(1, x));
    const s = v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055;
    return Math.round(s * 255);
  });
  return alfa >= 1 ? `rgb(${r},${g},${bl})` : `rgba(${r},${g},${bl},${alfa.toFixed(3)})`;
}

/** Odcienie serii: rowno rozlozone, bez fioletu z czarnej listy. */
export const ODCIENIE = [163, 245, 75, 25, 205, 120, 345, 50];
