import { Film, ShieldCheck } from "lucide-preact";
import type { PublicInfo } from "../../types";
import { Badge } from "../Badge";

interface UserHeaderProps {
  info: PublicInfo | null;
  onNavigate: (path: string) => void;
}

/** Sticky top navigation header for the public User Portal. */
export function UserHeader({ info, onNavigate }: UserHeaderProps) {
  return (
    <header class="sticky top-0 z-20 border-b border-slate-800/80 bg-slate-950/85 backdrop-blur-md">
      <div class="mx-auto flex max-w-5xl items-center justify-between gap-2.5 px-3.5 py-3 sm:gap-3 sm:px-6 sm:py-4">
        <div class="flex min-w-0 items-center gap-2.5 sm:gap-3">
          <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-indigo-500 to-violet-600 shadow-lg shadow-indigo-500/25">
            <Film class="h-5 w-5 text-white" />
          </div>
          <div class="min-w-0">
            <div class="flex min-w-0 items-center gap-2">
              <span class="truncate text-base font-semibold tracking-tight text-white">
                Mini Media Server
              </span>
              {info?.version && (
                <span class="shrink-0 rounded-md border border-slate-800 bg-slate-900 px-1.5 py-0.5 font-mono text-[11px] text-slate-400">
                  v{info.version}
                </span>
              )}
            </div>
            <p class="truncate text-xs text-slate-400">Self-hosted Stremio Addon</p>
          </div>
        </div>

        <div class="flex shrink-0 items-center gap-2.5">
          {info && info.retention_days > 0 && (
            <span class="hidden sm:inline-flex">
              <Badge tone="slate">Retention: {info.retention_days}d</Badge>
            </span>
          )}
          <a
            href="/admin"
            onClick={(e) => {
              e.preventDefault();
              onNavigate("/admin");
            }}
            class="inline-flex min-h-9 shrink-0 items-center gap-1.5 rounded-lg border border-slate-800 bg-slate-900/90 px-3 py-1.5 text-xs font-medium whitespace-nowrap text-slate-300 transition hover:border-slate-700 hover:bg-slate-800 hover:text-white"
          >
            <ShieldCheck class="h-3.5 w-3.5 text-indigo-400" />
            Admin Panel
          </a>
        </div>
      </div>
    </header>
  );
}
