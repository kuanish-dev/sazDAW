import type { JSX } from "solid-js";

export interface IconProps {
  /** Pixel size; 16 by default (the activity bar uses 24). */
  size?: number;
  class?: string;
}

function Svg(props: IconProps & { children: JSX.Element; filled?: boolean }) {
  return (
    <svg
      width={props.size ?? 16} height={props.size ?? 16} viewBox="0 0 16 16" aria-hidden="true"
      class={`shrink-0 ${props.class ?? ""}`}
      fill={props.filled ? "currentColor" : "none"} stroke={props.filled ? "none" : "currentColor"}
      stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"
    >
      {props.children}
    </svg>
  );
}

/** Tracks: rows of lanes with clips. */
export const IconTracks = (p: IconProps) => (
  <Svg {...p}><path d="M2 3.5h2.5M2 8h2.5M2 12.5h2.5" /><rect x="6.5" y="2.5" width="7.5" height="2" rx="0.5" /><rect x="8.5" y="7" width="5.5" height="2" rx="0.5" /><rect x="6.5" y="11.5" width="4.5" height="2" rx="0.5" /></Svg>
);

/** Graph: nodes joined by wires. */
export const IconGraph = (p: IconProps) => (
  <Svg {...p}><rect x="1.5" y="2" width="4" height="3.5" rx="0.6" /><rect x="1.5" y="10.5" width="4" height="3.5" rx="0.6" /><rect x="10.5" y="6.25" width="4" height="3.5" rx="0.6" /><path d="M5.5 3.75h1.5c1 0 1.5.5 1.5 1.5v1.5c0 .8.4 1.25 1.2 1.25h.8M5.5 12.25h1.5c1 0 1.5-.5 1.5-1.5V9.3" /></Svg>
);

/** Mixer: three faders. */
export const IconMixer = (p: IconProps) => (
  <Svg {...p}><path d="M3.5 2v12M8 2v12M12.5 2v12" /><rect x="2" y="8.5" width="3" height="2" rx="0.4" fill="currentColor" /><rect x="6.5" y="4" width="3" height="2" rx="0.4" fill="currentColor" /><rect x="11" y="10" width="3" height="2" rx="0.4" fill="currentColor" /></Svg>
);

/** Stage: instruments standing in an arc in front of the listener. */
export const IconStage = (p: IconProps) => (
  <Svg {...p}><circle cx="8" cy="12.6" r="1.7" /><path d="M2 9.6c1.8-1.5 3.8-2.3 6-2.3s4.2.8 6 2.3" stroke-dasharray="1.2 1.6" /><circle cx="3" cy="5.6" r="1.2" fill="currentColor" /><circle cx="8" cy="3" r="1.2" fill="currentColor" /><circle cx="13" cy="5.6" r="1.2" fill="currentColor" /></Svg>
);

/** Samples: a waveform. */
export const IconWave = (p: IconProps) => (
  <Svg {...p}><path d="M1.5 8h1M4 5.5v5M6.5 3v10M9 5v6M11.5 6.5v3M14 8h.5" /></Svg>
);

/** Settings: a gear. */
export const IconGear = (p: IconProps) => (
  <Svg {...p}>
    <path d="M6.20 3.23L6.49 1.27L9.51 1.27L9.80 3.23L10.10 3.35L11.70 2.17L13.83 4.30L12.65 5.90L12.77 6.20L14.73 6.49L14.73 9.51L12.77 9.80L12.65 10.10L13.83 11.70L11.70 13.83L10.10 12.65L9.80 12.77L9.51 14.73L6.49 14.73L6.20 12.77L5.90 12.65L4.30 13.83L2.17 11.70L3.35 10.10L3.23 9.80L1.27 9.51L1.27 6.49L3.23 6.20L3.35 5.90L2.17 4.30L4.30 2.17L5.90 3.35Z" />
    <circle cx="8" cy="8" r="2.2" />
  </Svg>
);

export const IconPlay = (p: IconProps) => <Svg {...p} filled><path d="M4.5 2.8v10.4c0 .5.5.8.9.5l8-5.2a.6.6 0 0 0 0-1L5.4 2.3c-.4-.3-.9 0-.9.5Z" /></Svg>;
export const IconPause = (p: IconProps) => <Svg {...p} filled><rect x="3.5" y="2.5" width="3" height="11" rx="0.6" /><rect x="9.5" y="2.5" width="3" height="11" rx="0.6" /></Svg>;
export const IconStop = (p: IconProps) => <Svg {...p} filled><rect x="3" y="3" width="10" height="10" rx="1" /></Svg>;
export const IconRecord = (p: IconProps) => <Svg {...p} filled><circle cx="8" cy="8" r="4.5" /></Svg>;
export const IconMetronome = (p: IconProps) => (
  <Svg {...p}><path d="M6.2 1.8h3.6l3 12.2H3.2Z" /><path d="M4.4 10.5h7.2M8 10.5l3.6-6.2" /></Svg>
);
export const IconUndo = (p: IconProps) => <Svg {...p}><path d="M5 3.5 2.5 6 5 8.5" /><path d="M2.5 6H10a3.5 3.5 0 0 1 0 7H6" /></Svg>;
export const IconRedo = (p: IconProps) => <Svg {...p}><path d="M11 3.5 13.5 6 11 8.5" /><path d="M13.5 6H6a3.5 3.5 0 0 0 0 7h4" /></Svg>;
/** A clock with a turn-back arrow: the edit history. */
export const IconHistory = (p: IconProps) => (
  <Svg {...p}><path d="M2.5 8a5.5 5.5 0 1 0 1.6-3.9" /><path d="M4.1 1.8v2.3h2.3" /><path d="M8 5v3.2l2.2 1.3" /></Svg>
);
/** A four-point spark: something the AI made. */
export const IconSparkle = (p: IconProps) => (
  <Svg {...p} filled><path d="M8 1.5C8.5 5.4 10.6 7.5 14.5 8 10.6 8.5 8.5 10.6 8 14.5 7.5 10.6 5.4 8.5 1.5 8 5.4 7.5 7.5 5.4 8 1.5Z" /></Svg>
);

export const IconChevronDown = (p: IconProps) => <Svg {...p}><path d="M4.5 6.5 8 10l3.5-3.5" /></Svg>;
export const IconChevronRight = (p: IconProps) => <Svg {...p}><path d="M6.5 4.5 10 8l-3.5 3.5" /></Svg>;
export const IconClose = (p: IconProps) => <Svg {...p}><path d="M4 4l8 8M12 4l-8 8" /></Svg>;
export const IconAdd = (p: IconProps) => <Svg {...p}><path d="M8 3v10M3 8h10" /></Svg>;
export const IconArrowUp = (p: IconProps) => <Svg {...p}><path d="M8 13V3M4 7l4-4 4 4" /></Svg>;
export const IconFolder = (p: IconProps) => (
  <Svg {...p}><path d="M1.5 4c0-.6.4-1 1-1h3.3l1.5 1.5h6.2c.6 0 1 .4 1 1V12c0 .6-.4 1-1 1H2.5c-.6 0-1-.4-1-1Z" /></Svg>
);
export const IconNote = (p: IconProps) => <Svg {...p}><path d="M6 12V3.5l7-1.5v8.5" /><circle cx="4.3" cy="12" r="1.7" /><circle cx="11.3" cy="10.5" r="1.7" /></Svg>;
export const IconError = (p: IconProps) => <Svg {...p}><circle cx="8" cy="8" r="6" /><path d="M5.8 5.8l4.4 4.4M10.2 5.8l-4.4 4.4" /></Svg>;
export const IconPulse = (p: IconProps) => <Svg {...p}><path d="M1.5 8.5h3l1.5-4 2.5 8 2-6 1 2h3" /></Svg>;

export const IconPerson = (p: IconProps) => <Svg {...p}><circle cx="8" cy="5" r="2.8" /><path d="M2.5 14c.4-3 2.6-4.5 5.5-4.5s5.1 1.5 5.5 4.5" /></Svg>;
export const IconPaint = (p: IconProps) => (
  <Svg {...p}><path d="M8 1.8a6.2 6.2 0 1 0 0 12.4c1 0 1.4-.6 1.2-1.4-.3-1 .2-1.8 1.3-1.8h1.6c1.4 0 2.1-1 2.1-2.4A6.4 6.4 0 0 0 8 1.8Z" /><circle cx="5" cy="7" r=".8" /><circle cx="7.5" cy="4.6" r=".8" /><circle cx="10.6" cy="5.4" r=".8" /></Svg>
);
export const IconSpeaker = (p: IconProps) => (
  <Svg {...p}><path d="M2 6h2.5L8 3v10l-3.5-3H2Z" /><path d="M10.5 5.5a3.5 3.5 0 0 1 0 5M12.3 3.5a6.3 6.3 0 0 1 0 9" /></Svg>
);
export const IconHeadphones = (p: IconProps) => (
  <Svg {...p}><path d="M2.5 10.5V8a5.5 5.5 0 0 1 11 0v2.5" /><rect x="1.8" y="9.5" width="3" height="4.5" rx="1" /><rect x="11.2" y="9.5" width="3" height="4.5" rx="1" /></Svg>
);
export const IconPiano = (p: IconProps) => (
  <Svg {...p}><rect x="1.5" y="3" width="13" height="10" rx="1" /><path d="M5.8 3v10M10.2 3v10" /><path d="M4.4 3v5.5M7.2 3v5.5M8.8 3v5.5M11.6 3v5.5" stroke-width="2" /></Svg>
);
export const IconPlug = (p: IconProps) => (
  <Svg {...p}><rect x="2" y="2" width="5" height="5" rx="0.6" /><rect x="9" y="2" width="5" height="5" rx="0.6" /><rect x="2" y="9" width="5" height="5" rx="0.6" /><path d="M11.5 9v5M9 11.5h5" /></Svg>
);
export const IconLibrary = (p: IconProps) => <Svg {...p}><path d="M2.5 2.5v11M5.5 2.5v11M8.5 3.5l3.8 10" /><path d="M1.5 13.5h13" /></Svg>;
export const IconKeyboard = (p: IconProps) => (
  <Svg {...p}><rect x="1" y="3.5" width="14" height="9" rx="1" /><path d="M3.5 6.5h1M6 6.5h1M8.5 6.5h1M11 6.5h1.5M3.5 9h1M11.5 9h1M5.5 10.5h5" /></Svg>
);
export const IconInfo = (p: IconProps) => <Svg {...p}><circle cx="8" cy="8" r="6" /><path d="M8 7.2v4M8 4.8v.1" /></Svg>;

export const IconPencil = (p: IconProps) => <Svg {...p}><path d="M10.5 2.5l3 3-8 8H2.5v-3Z" /><path d="M9 4l3 3" /></Svg>;
export const IconScissors = (p: IconProps) => (
  <Svg {...p}><circle cx="4" cy="12" r="2" /><circle cx="12" cy="12" r="2" /><path d="M5.4 10.6 12.5 2M10.6 10.6 3.5 2" /></Svg>
);
export const IconEraser = (p: IconProps) => (
  <Svg {...p}><path d="M6 13.5 1.8 9.3a1 1 0 0 1 0-1.4l6-6a1 1 0 0 1 1.4 0l4.9 4.9a1 1 0 0 1 0 1.4L8.4 13.5Z" /><path d="M5 5.2l5.8 5.8M6 13.5h8" /></Svg>
);
/** Select tool: an arrow pointer. */
export const IconPointer = (p: IconProps) => <Svg {...p}><path d="M3.5 2v11l3-3 2 4.5 1.8-.8-2-4.4h4.2Z" /></Svg>;
export const IconFlag = (p: IconProps) => <Svg {...p}><path d="M3.5 14.5V2M3.5 2.5h8.5l-2 3 2 3H3.5" /></Svg>;
export const IconLoop = (p: IconProps) => (
  <Svg {...p}><path d="M2.5 8a4 4 0 0 1 4-4h6M10.5 2l2 2-2 2M13.5 8a4 4 0 0 1-4 4h-6M5.5 14l-2-2 2-2" /></Svg>
);
export const IconDrum = (p: IconProps) => (
  <Svg {...p}><ellipse cx="8" cy="6" rx="5.5" ry="2" /><path d="M2.5 6v5c0 1.1 2.5 2 5.5 2s5.5-.9 5.5-2V6M5 7.8v5M11 7.8v5M9.5 1.5l3 3M6.5 1.5l-3 3" /></Svg>
);
/** Guitar: the body with its sound hole and the neck to the top right. */
export const IconGuitar = (p: IconProps) => (
  <Svg {...p}><circle cx="6" cy="10" r="3.8" /><circle cx="6" cy="10" r="0.9" /><path d="M8.7 7.3 13.5 2.5M12.3 1.5l2.2 2.2" /></Svg>
);
export const IconMinus = (p: IconProps) => <Svg {...p}><path d="M3 8h10" /></Svg>;
export const IconZoomIn = (p: IconProps) => <Svg {...p}><circle cx="7" cy="7" r="4.5" /><path d="M10.3 10.3 14 14M5 7h4M7 5v4" /></Svg>;
export const IconZoomOut = (p: IconProps) => <Svg {...p}><circle cx="7" cy="7" r="4.5" /><path d="M10.3 10.3 14 14M5 7h4" /></Svg>;
/** Fit: corners of a frame. */
export const IconFit = (p: IconProps) => <Svg {...p}><path d="M2 5.5V2h3.5M10.5 2H14v3.5M14 10.5V14h-3.5M5.5 14H2v-3.5" /></Svg>;
export const IconTrash = (p: IconProps) => (
  <Svg {...p}><path d="M2.5 4h11M6 4V2.5h4V4M4 4l.7 9.5h6.6L12 4M6.8 6.5v4.5M9.2 6.5v4.5" /></Svg>
);
export const IconCopy = (p: IconProps) => <Svg {...p}><rect x="5" y="5" width="8.5" height="8.5" rx="1" /><path d="M11 5V3.5a1 1 0 0 0-1-1H3.5a1 1 0 0 0-1 1V10a1 1 0 0 0 1 1H5" /></Svg>;
export const IconPaste = (p: IconProps) => (
  <Svg {...p}><path d="M5.5 2.5h-2a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h9a1 1 0 0 0 1-1v-10a1 1 0 0 0-1-1h-2" /><rect x="5.5" y="1.5" width="5" height="2.5" rx="0.6" /></Svg>
);
/** Duplicate: a clip and its copy right after it. */
export const IconDuplicate = (p: IconProps) => <Svg {...p}><rect x="1.5" y="5" width="6" height="6" rx="0.8" /><rect x="8.5" y="5" width="6" height="6" rx="0.8" stroke-dasharray="2 1.4" /></Svg>;
export const IconImport = (p: IconProps) => <Svg {...p}><path d="M8 2v8M4.5 6.5 8 10l3.5-3.5M2.5 11v2.5h11V11" /></Svg>;
export const IconExport = (p: IconProps) => <Svg {...p}><path d="M8 10V2M4.5 5.5 8 2l3.5 3.5M2.5 11v2.5h11V11" /></Svg>;

/** Drag handle: two columns of dots. */
export const IconGripper = (p: IconProps) => (
  <Svg {...p} filled><circle cx="6" cy="4" r="1" /><circle cx="10" cy="4" r="1" /><circle cx="6" cy="8" r="1" /><circle cx="10" cy="8" r="1" /><circle cx="6" cy="12" r="1" /><circle cx="10" cy="12" r="1" /></Svg>
);
/** Instrument parameters: three knobs-on-sliders. */
export const IconSliders = (p: IconProps) => (
  <Svg {...p}><path d="M2 4h3.5M9 4h5M2 8h7.5M13 8h1M2 12h2M7.5 12h6.5" /><circle cx="7.2" cy="4" r="1.6" /><circle cx="11.2" cy="8" r="1.6" /><circle cx="5.8" cy="12" r="1.6" /></Svg>
);
export const IconSnowflake = (p: IconProps) => (
  <Svg {...p}><path d="M8 1.5v13M2.4 4.75l11.2 6.5M2.4 11.25l11.2-6.5M6.3 2.5 8 4l1.7-1.5M6.3 13.5 8 12l1.7 1.5" /></Svg>
);
/** Bypass / on-off. */
export const IconPower = (p: IconProps) => <Svg {...p}><path d="M8 1.8v5.4" /><path d="M4.7 4a5 5 0 1 0 6.6 0" /></Svg>;
export const IconArrowDown = (p: IconProps) => <Svg {...p}><path d="M8 3v10M4 9l4 4 4-4" /></Svg>;
export const IconCheck = (p: IconProps) => <Svg {...p}><path d="M3 8.5 6.5 12 13 4.5" /></Svg>;
export const IconDice = (p: IconProps) => (
  <Svg {...p}><rect x="2" y="2" width="12" height="12" rx="2" /><circle cx="5.5" cy="5.5" r=".9" fill="currentColor" /><circle cx="10.5" cy="10.5" r=".9" fill="currentColor" /><circle cx="8" cy="8" r=".9" fill="currentColor" /></Svg>
);
export const IconRefresh = (p: IconProps) => <Svg {...p}><path d="M13 8a5 5 0 1 1-1.5-3.5M13 2v3h-3" /></Svg>;
/** Piano roll view of a clip. */
export const IconRoll = (p: IconProps) => <Svg {...p}><path d="M2 3h3M6.5 6.5h4M4 10h3.5M9.5 13h4.5" stroke-width="2" /></Svg>;
/** Step view of a clip. */
export const IconSteps = (p: IconProps) => (
  <Svg {...p}><rect x="1.5" y="5.5" width="3" height="5" rx="0.5" fill="currentColor" /><rect x="6.5" y="5.5" width="3" height="5" rx="0.5" /><rect x="11.5" y="5.5" width="3" height="5" rx="0.5" fill="currentColor" /></Svg>
);
export const IconArrowRight = (p: IconProps) => <Svg {...p}><path d="M3 8h10M9 4l4 4-4 4" /></Svg>;
