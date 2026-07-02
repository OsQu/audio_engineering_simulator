// ============================================================================
// Audio Engineer Simulator — Design Tokens (TypeScript mirror)
// ----------------------------------------------------------------------------
// Hand-written TS mirror of tokens.css. Same role catalog.ts plays for the Rust
// catalog: the values live in both places, kept in sync by hand. Use these when
// you need a token in *logic* (drawing a cable on a canvas, picking a jack ring
// color from a port `kind`, tinting a meter) rather than in CSS.
//
// The CSS file is the source of truth for *styling*; this file is the source of
// truth for *computation*. If you change one, change the other.
// ============================================================================

/** The six signal kinds carried by ports and cables. Mirrors the port `kind`
 *  union in catalog.ts. */
export type SignalKind = "mic" | "line" | "instrument" | "speaker" | "digital" | "midi";

export interface SignalColor {
  /** Canonical color — the cable core, the jack ring. */
  base: string;
  /** Highlight — gradient top, the hot inner wire, the lit center. */
  lit: string;
  /** Soft halo around a live jack / cable end. */
  glow: string;
}

/** Port + cable color = what the port carries. Index by a port's `kind`. */
export const SIGNAL: Record<SignalKind, SignalColor> = {
  mic: { base: "#2f6fc0", lit: "#6fb0ef", glow: "rgba(74,144,217,0.45)" },
  line: { base: "#9aa0a6", lit: "#e9edf1", glow: "rgba(190,196,202,0.35)" },
  instrument: { base: "#d2762a", lit: "#f2ac6a", glow: "rgba(224,138,60,0.45)" },
  speaker: { base: "#c5392c", lit: "#ef6f66", glow: "rgba(214,69,60,0.45)" },
  digital: { base: "#1f9c8d", lit: "#5fd6c6", glow: "rgba(43,179,163,0.45)" },
  midi: { base: "#7e54c8", lit: "#b692ea", glow: "rgba(155,108,214,0.45)" },
} as const;

/** Surfaces & chrome around the gear. */
export const SURFACE = {
  bgRoom: "#0a0b0d",
  bgPanel: "#15171b",
  bgPanel2: "#101216",
  bgCard: "#1a1d22",
  bgCard2: "#131519",
  bgChip: "#1b1e23",
  lineCard: "#25282e",
  linePanel: "#24272d",
  lineChip: "#2c3036",
  lineHard: "#0a0b0d",
} as const;

/** Text colors. */
export const TEXT = {
  primary: "#f3f5f7",
  strong: "#dfe3e7",
  secondary: "#aeb6bf",
  muted: "#7e8794",
  faint: "#566069",
  readout: "#5fd6c6",
} as const;

export interface LedColor {
  /** Edge / off-center body of the lit lamp. */
  base: string;
  /** Lit center. */
  lit: string;
  /** Emission halo. */
  glow: string;
  /** The same lamp, dark/unlit. */
  off: string;
}

/** Indicator LEDs (state lights — distinct from signal jacks). */
export const LED: Record<"green" | "amber" | "red", LedColor> = {
  green: { base: "#1f9b3f", lit: "#9bffb4", glow: "rgba(60,220,110,0.85)", off: "#1a2417" },
  amber: { base: "#dc9b2f", lit: "#ffe39a", glow: "rgba(240,180,60,0.80)", off: "#241a1a" },
  red: { base: "#c5392c", lit: "#ff8d83", glow: "rgba(230,70,60,0.80)", off: "#241a1a" },
} as const;

/** A neutral dark lamp for any LED in its off state on a generic surface. */
export const LED_NEUTRAL_OFF = "#121316";

export interface CapFinish {
  /** Cap gradient top stop. */
  top: string;
  /** Cap gradient bottom stop. */
  bot: string;
  /** Pointer/notch color that reads against this cap. */
  pointer: string;
}

/** Knob cap finishes. The brushed-metal collar around them is shared (see METAL). */
export const KNOB_CAP: Record<"dark" | "chrome" | "red" | "blue" | "cream", CapFinish> = {
  dark: { top: "#3e4248", bot: "#16181b", pointer: "#f1f1ef" },
  chrome: { top: "#d4d8dc", bot: "#82888e", pointer: "#1a1a1a" },
  red: { top: "#d8584a", bot: "#7a1810", pointer: "#ffffff" },
  blue: { top: "#5b8fd6", bot: "#1f3f7a", pointer: "#ffffff" },
  cream: { top: "#e6e3da", bot: "#aaa597", pointer: "#1a1a1a" },
} as const;

/** Shared brushed-metal collar stops + tick color. */
export const METAL = {
  collar: ["#e8ebee", "#b0b6bc", "#7a8087", "#565b61"] as const,
  tick: "#9aa0a6",
} as const;

/** Fader hardware. */
export const FADER = {
  slotEdge: "#070809",
  slotMid: "#2a2d31",
  capTop: "#43464c",
  capBot: "#191b1e",
  index: "#e9edf1",
  capMasterTop: "#c43b32",
  capMasterBot: "#7a1810",
} as const;

/** Jack barrel (recessed connector body). The ring color is a SIGNAL.base. */
export const JACK = {
  top: "#3a3d42",
  bot: "#0c0d10",
  edge: "#050608",
} as const;

/** VU meter face + movement. */
export const VU = {
  faceTop: "#f3ecd4",
  faceBot: "#e3d8b4",
  ink: "#3a3527",
  red: "#b23a2a",
  needle: "#1a1a1a",
  pivot: "#2a2a2a",
  bezel1: "#0c0d0f",
  bezel2: "#1c1e21",
} as const;

export interface PanelFinish {
  /** Faceplate gradient stops, top → bottom (apply at 165deg). */
  face: readonly [string, string, string];
  /** Matching rack-ear / side color. */
  ear: string;
}

/** Device faceplate finishes. */
export const PANEL_FINISH: Record<"grey" | "slate" | "black", PanelFinish> = {
  grey: { face: ["#c8cac3", "#b0b2aa", "#9a9c94"], ear: "#92948c" }, // vintage opto (LA-2A)
  slate: { face: ["#3c4855", "#2e3947", "#26303c"], ear: "#1c222b" }, // slate-blue pre (Neve)
  black: { face: ["#222420", "#161712", "#0f100c"], ear: "#0c0d0f" }, // matte black (Distressor)
} as const;

/** Rack cabinet + hardware. */
export const RACK = {
  shell: ["#202327", "#14161a"] as const,
  rail: "#191b1e",
  hole: "#050608",
  screw: ["#eef1f3", "#8a9097", "#4a4f55"] as const,
  ventDark: "#0c0d0f",
  ventLite: "#2a2d31",
  glow: "rgba(255,205,140,0.14)",
} as const;

/** Typography. */
export const TYPE = {
  fontDisplay: '"Space Grotesk", system-ui, sans-serif',
  fontUi: '"Inter", system-ui, sans-serif',
  labelSize: "9px",
  labelWeight: 600,
  labelSpacing: "0.09em",
  legendSpacing: "0.16em",
  valueSize: "9px",
} as const;

/** Elevation, bevels and edges. */
export const ELEVATION = {
  card: "0 16px 34px rgba(0,0,0,0.55)",
  rack: "0 28px 56px rgba(0,0,0,0.60)",
  knob: "0 2px 4px rgba(0,0,0,0.55)",
  control: "0 2px 4px rgba(0,0,0,0.50)",
  bevelTop: "inset 0 1px 0 rgba(255,255,255,0.06)",
  bevelPress: "inset 0 1px 2px rgba(0,0,0,0.70)",
} as const;

/** Corner radii (px). */
export const RADIUS = {
  card: 12,
  panel: 6,
  control: 5,
  pill: 999,
} as const;

/** Spacing scale (px). */
export const SPACE = [4, 7, 12, 18, 26] as const;

/** Patch-cable rendering constants. Color is the matching SIGNAL entry. */
export const CABLE = {
  shadow: "#0a0d12",
  shadowWidth: 9,
  coreWidth: 6,
  highlightWidth: 2,
  /** How far the bezier control points droop below the endpoints (px). */
  sag: 90,
} as const;

/** Top-view / floor-plan routing trails. */
export const ROUTE = {
  /** SVG stroke-dasharray for a dashed signal trail. */
  dash: "2 7",
  arrow: "#cfd3d8",
} as const;

// ----------------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------------

/** Brushed-metal collar gradient (the ring around a knob). */
export function collarGradient(): string {
  const [a, b, c, d] = METAL.collar;
  return `radial-gradient(circle at 50% 30%, ${a}, ${b} 48%, ${c} 78%, ${d} 100%)`;
}

/** Knob cap gradient for a given finish. */
export function capGradient(finish: keyof typeof KNOB_CAP): string {
  const { top, bot } = KNOB_CAP[finish];
  return `radial-gradient(circle at 50% 28%, ${top}, ${bot} 74%)`;
}

/** A lit LED's radial fill. */
export function ledFill(color: keyof typeof LED): string {
  const c = LED[color];
  return `radial-gradient(circle at 40% 35%, ${c.lit}, ${c.base})`;
}

/** A lit LED's box-shadow glow (compose with any inset highlights you want). */
export function ledGlow(color: keyof typeof LED, px = 10): string {
  return `0 0 ${px}px ${LED[color].glow}`;
}

/** Device faceplate gradient for a finish. */
export function faceplateGradient(finish: keyof typeof PANEL_FINISH): string {
  const [a, b, c] = PANEL_FINISH[finish].face;
  return `linear-gradient(165deg, ${a}, ${b} 55%, ${c})`;
}

/** Jack barrel fill (the recessed body; draw the signal ring on top). */
export function jackBarrel(): string {
  return `radial-gradient(circle at 50% 32%, ${JACK.top}, ${JACK.bot})`;
}
