const SUPPORTS_TRUECOLOR =
  process.env.COLORTERM === "truecolor" || process.env.COLORTERM === "24bit";
const LEVELS = [0, 95, 135, 175, 215, 255];

function nearestLevel(value: number): number {
  let best = 0;
  let bestDistance = Number.POSITIVE_INFINITY;

  for (let index = 0; index < LEVELS.length; index += 1) {
    const distance = Math.abs(value - LEVELS[index]);
    if (distance < bestDistance) {
      bestDistance = distance;
      best = index;
    }
  }

  return best;
}

export const BRAND = {
  primary: "#F26424",
  secondary: "#F8A76F",
  hint: "#E98B5B",
  highlight: "#F2A077",
  error: "#CC3219",
  errorAlt: "#9A3B32",
  critical: "#8E251E",
} as const;

export const BRAND_SPINNER_FRAMES = ["✱", "✦", "✧", "⋆"] as const;
export const WAVE_CHARS = "▁▂▃▄▅▆▇█▇▆▅▄▃▂▁";
export const LOGO_LINES = [
  "⠀⠀⠀⠀⠀⠀⠀⢀⣠⣴⣶⣶⣿⣿⣿⣿⣶⣶⣦⣄⡀⠀⠀⠀⠀⠀⠀⠀",
  "⠀⠀⠀⠀⢀⣤⣾⡿⠟⠋⠉⠀⠀⠀⠀⠀⠀⠉⠙⠻⢿⣷⣤⡀⠀⠀⠀⠀",
  "⠀⠀⢀⣴⣿⠟⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠻⣿⣦⡀⠀⠀",
  "⠀⢀⣾⡿⠁⣀⣤⣴⣶⣾⣿⡿⠿⠿⠿⠿⢿⣿⣷⣶⣦⣤⣀⠈⢿⣷⡀⠀",
  "⢀⣾⣿⣶⣿⠿⠛⠉⠁⠀⢀⣄⠀⠀⠀⠀⣠⡀⠀⠈⠉⠛⠿⣿⣾⣿⣷⡀",
  "⣸⣿⡿⠋⠀⠀⠀⠀⠀⠀⠻⣿⣧⡀⢀⣼⣿⠟⠀⠀⠀⠀⠀⠀⠙⢿⣿⡇",
  "⣿⣿⠁⠀⠀⠀⠀⠀⢠⣤⣤⣼⣿⣿⣿⣿⣧⣤⣤⡄⠀⠀⠀⠀⠀⠈⣿⣿",
  "⣿⣿⡀⠀⠀⠀⠀⠀⠘⠛⠛⣻⣿⡿⢿⣿⣟⠛⠛⠃⠀⠀⠀⠀⠀⢠⣿⡿",
  "⢸⣿⣿⣦⣀⠀⠀⠀⠀⠀⣾⣿⠟⠁⠈⠻⣿⡷⠀⠀⠀⠀⠀⣀⣴⣿⣿⡇",
  "⠀⢿⣿⡛⠿⣷⣶⣤⣄⣀⣀⠁⠀⠀⠀⠀⠈⣀⣀⣠⣤⣶⣾⠿⢛⣿⡟⠀",
  "⠀⠈⢻⣿⣄⠀⠉⠙⠛⠻⠿⠿⠿⠿⠿⠿⠿⠿⠟⠛⠋⠉⠀⢠⣿⡟⠀⠀",
  "⠀⠀⠀⠙⢿⣷⣄⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⣿⣧⠀⠀",
  "⠀⠀⠀⠀⠀⠙⠻⣿⣶⣤⣄⣀⣀⣀⣀⣀⣀⣠⣤⣶⣷⣶⣤⣀⣸⣿⡆⠀",
  "⠀⠀⠀⠀⠀⠀⠀⠀⠉⠙⠛⠻⠿⠿⠿⠿⠟⠛⠋⠁⠀⠉⠛⠻⠿⣿⡟⠀",
] as const;

export function hex(hexColor: string, text: string): string {
  const r = Number.parseInt(hexColor.slice(1, 3), 16);
  const g = Number.parseInt(hexColor.slice(3, 5), 16);
  const b = Number.parseInt(hexColor.slice(5, 7), 16);

  if (SUPPORTS_TRUECOLOR) {
    return `\x1b[38;2;${r};${g};${b}m${text}\x1b[39m`;
  }

  const index =
    16 +
    36 * nearestLevel(r) +
    6 * nearestLevel(g) +
    nearestLevel(b);
  return `\x1b[38;5;${index}m${text}\x1b[39m`;
}

