import type { ComponentChildren } from "preact";

export type BadgeTone = "emerald" | "indigo" | "amber" | "rose" | "slate";

const TONE_CLASSES: Record<BadgeTone, string> = {
  emerald: "bg-emerald-500/15 text-emerald-300 border-emerald-500/30",
  indigo: "bg-indigo-500/15 text-indigo-300 border-indigo-500/30",
  amber: "bg-amber-500/15 text-amber-300 border-amber-500/30",
  rose: "bg-rose-500/15 text-rose-300 border-rose-500/30",
  slate: "bg-slate-800/80 text-slate-300 border-slate-700",
};

interface BadgeProps {
  tone?: BadgeTone;
  children: ComponentChildren;
  className?: string;
}

export function Badge({ tone = "slate", children, className = "" }: BadgeProps) {
  return (
    <span
      class={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-medium ${TONE_CLASSES[tone]} ${className}`}
    >
      {children}
    </span>
  );
}
