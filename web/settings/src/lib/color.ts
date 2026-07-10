// 颜色互转与五行主色。

/// project.json 颜色是 "r g b"（0..1 浮点），转 `<input type=color>` 的 #rrggbb。
export function rgbStrToHex(s: string): string {
  const parts = s.split(/\s+/).map(Number);
  const [r = 0, g = 0, b = 0] = parts;
  const h = (n: number) =>
    Math.round(Math.max(0, Math.min(1, n)) * 255)
      .toString(16)
      .padStart(2, '0');
  return `#${h(r)}${h(g)}${h(b)}`;
}

/// #rrggbb → "r g b"（0..1 浮点）。
export function hexToRgbStr(hex: string): string {
  const n = parseInt(hex.slice(1), 16);
  const r = ((n >> 16) & 255) / 255;
  const g = ((n >> 8) & 255) / 255;
  const b = (n & 255) / 255;
  return `${r} ${g} ${b}`;
}

/// 日干 → 五行主色（甲乙木青/丙丁火赤/戊己土黄/庚辛金白/壬癸水玄）。
export function accentFor(gan: string): string {
  const wood = '#4fd1c5';
  const fire = '#f56565';
  const earth = '#ecc94b';
  const metal = '#e2e8f0';
  const water = '#5a8dd6';
  const map: Record<string, string> = {
    甲: wood, 乙: wood,
    丙: fire, 丁: fire,
    戊: earth, 己: earth,
    庚: metal, 辛: metal,
    壬: water, 癸: water,
  };
  return map[gan] ?? wood;
}
