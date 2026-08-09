import type { CSSProperties } from "react";

/** Claudeix C+X glyph. Keep this component name so the Pi Desktop shell can
 * retain its original layout/import graph while the branding is replaced. */
const GLYPH = (
  <>
    <path
      d="M708 314 C654 248 574 216 482 216 C306 216 198 340 198 512 C198 684 306 808 482 808 C574 808 654 776 708 710"
      fill="none"
      stroke="currentColor"
      strokeWidth={96}
      strokeLinecap="round"
    />
    <g
      fill="none"
      stroke="var(--accent, #7c83ff)"
      strokeWidth={74}
      strokeLinecap="round"
    >
      <path d="M674 430 L826 594" />
      <path d="M826 430 L674 594" />
    </g>
  </>
);

/**
 * Global Claudeix brand mark, rendered inside the original Pi Desktop shell.
 *
 * - Default: monochrome glyph inheriting `currentColor` — for nav rails,
 *   status bars, and any inline small-size use.
 * - `withBackground`: warm-paper rounded tile matching the app icon.
 */
export function PiMark({
  size = 20,
  withBackground = false,
  style,
  title = "Claudeix",
}: {
  size?: number;
  withBackground?: boolean;
  style?: CSSProperties;
  title?: string;
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 1024 1024"
      role="img"
      aria-label={title}
      style={style}
    >
      {withBackground && (
        <rect width={1024} height={1024} rx={224} fill="#181b22" />
      )}
      <g color={withBackground ? "#f7f8fb" : undefined}>{GLYPH}</g>
    </svg>
  );
}
