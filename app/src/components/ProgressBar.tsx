interface ProgressBarProps {
  progressPct: number;
  className?: string;
}

/** Horizontal torrent completion progress bar. */
export function ProgressBar({ progressPct, className = "" }: ProgressBarProps) {
  const pct = Math.min(100, Math.max(0, progressPct));
  return (
    <div class={`mt-3 h-2 w-full overflow-hidden rounded-full bg-slate-800 sm:h-1.5 ${className}`}>
      <div
        class={`h-full rounded-full transition-all duration-500 ${
          pct >= 100 ? "bg-emerald-500" : "bg-indigo-500"
        }`}
        style={{ width: `${pct}%` }}
      />
    </div>
  );
}
