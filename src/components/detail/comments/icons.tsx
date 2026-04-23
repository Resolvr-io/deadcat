/**
 * Note-action icons for the comment row, matching the visual style of
 * Ditto's PostActionBar. Stroke paths mirror Lucide so the set reads as
 * a family. Icons are decorative — each sits inside a button with a
 * `title`, so `aria-hidden` is applied here.
 */

import type { ReactNode } from "react";

type IconProps = {
  className?: string;
  strokeWidth?: number;
};

function Frame({
  className,
  strokeWidth = 2,
  children,
}: IconProps & { children: ReactNode }) {
  return (
    <svg
      aria-hidden="true"
      xmlns="http://www.w3.org/2000/svg"
      width={20}
      height={20}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
    >
      {children}
    </svg>
  );
}

export function ReplyIcon(props: IconProps) {
  return (
    <Frame {...props}>
      <path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z" />
    </Frame>
  );
}

export function HeartIcon(props: IconProps) {
  return (
    <Frame {...props}>
      <path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z" />
    </Frame>
  );
}

export function ZapIcon(props: IconProps) {
  return (
    <Frame {...props}>
      <path d="M4 14a1 1 0 0 1-.78-1.63l9.9-10.2a.5.5 0 0 1 .86.46l-1.92 6.02A1 1 0 0 0 13 10h7a1 1 0 0 1 .78 1.63l-9.9 10.2a.5.5 0 0 1-.86-.46l1.92-6.02A1 1 0 0 0 11 14z" />
    </Frame>
  );
}

export function TrashIcon(props: IconProps) {
  return (
    <Frame {...props}>
      <path d="M3 6h18" />
      <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
      <path d="M10 11v6" />
      <path d="M14 11v6" />
    </Frame>
  );
}
