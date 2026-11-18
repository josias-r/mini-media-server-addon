import { Activity, ArrowDown, ArrowRight, ArrowUp, HardDrive, Server, Users } from "lucide-preact";
import type { AdminConfigInfo, AdminStats } from "../../types";
import { formatBytes, formatSpeed } from "../../utils";

interface AdminKpiGridProps {
  stats: AdminStats;
  config: AdminConfigInfo | undefined;
  onNavigate: (path: string) => void;
}

/** Four-card KPI summary grid for the Admin Overview page. */
export function AdminKpiGrid({ stats, config, onNavigate }: AdminKpiGridProps) {
  return (
    <section class="grid grid-cols-2 gap-2.5 sm:gap-4 lg:grid-cols-4">
      <div class="flex flex-col justify-between rounded-2xl border border-slate-800 bg-slate-900/70 p-3.5 sm:p-4">
        <div>
          <div class="flex items-center justify-between text-xs font-medium text-slate-400">
            <span>Active Users</span>
            <Users class="h-4 w-4 shrink-0 text-indigo-400" />
          </div>
          <p class="mt-2 text-xl font-bold text-white tabular-nums sm:text-2xl">
            {stats.userCount}
          </p>
          <p class="mt-1 text-xs text-slate-400 tabular-nums">
            {stats.totalStreams} total stream
            {stats.totalStreams === 1 ? "" : "s"} recorded
          </p>
        </div>
        <a
          href="/admin/users"
          onClick={(e) => {
            e.preventDefault();
            onNavigate("/admin/users");
          }}
          class="mt-2.5 inline-flex items-center gap-1 text-xs font-medium text-indigo-400 transition hover:text-indigo-300"
        >
          Manage Users
          <ArrowRight class="h-3 w-3" />
        </a>
      </div>

      <div class="rounded-2xl border border-slate-800 bg-slate-900/70 p-3.5 sm:p-4">
        <div class="flex items-center justify-between text-xs font-medium text-slate-400">
          <span>Storage Used</span>
          <HardDrive class="h-4 w-4 shrink-0 text-violet-400" />
        </div>
        <p class="mt-2 text-xl font-bold text-white tabular-nums sm:text-2xl">
          {formatBytes(stats.totalBytes)}
        </p>
        <p class="mt-1 text-xs text-slate-400 tabular-nums">
          Across {stats.torrentCount} torrent
          {stats.torrentCount === 1 ? "" : "s"}
        </p>
      </div>

      <div class="rounded-2xl border border-slate-800 bg-slate-900/70 p-3.5 sm:p-4">
        <div class="flex items-center justify-between text-xs font-medium text-slate-400">
          <span>Live Bandwidth</span>
          <Activity class="h-4 w-4 shrink-0 text-emerald-400" />
        </div>
        <div class="mt-2 flex flex-wrap items-baseline gap-x-2.5 gap-y-0.5 tabular-nums">
          <span class="inline-flex items-center gap-1 text-base font-bold text-emerald-300 sm:text-lg">
            <ArrowDown class="h-3.5 w-3.5 sm:h-4 sm:w-4" />
            {formatSpeed(stats.dlSpeed)}
          </span>
          <span class="inline-flex items-center gap-1 text-xs font-medium text-slate-400 sm:text-sm">
            <ArrowUp class="h-3 w-3 sm:h-3.5 sm:w-3.5" />
            {formatSpeed(stats.upSpeed)}
          </span>
        </div>
        <p class="mt-1 text-xs text-slate-400 tabular-nums">
          {stats.downloadingCount} active download
          {stats.downloadingCount === 1 ? "" : "s"}
        </p>
      </div>

      <div class="rounded-2xl border border-slate-800 bg-slate-900/70 p-3.5 sm:p-4">
        <div class="flex items-center justify-between text-xs font-medium text-slate-400">
          <span>Policy &amp; Indexer</span>
          <Server class="h-4 w-4 shrink-0 text-amber-400" />
        </div>
        <p class="mt-2 text-xl font-bold text-white tabular-nums sm:text-2xl">
          {config?.retention_days ? `${config.retention_days} Days` : "Unlimited"}
        </p>
        <p class="mt-1 text-xs text-slate-400">
          Retention • Search:{" "}
          <span class="font-mono text-slate-300">{config?.jackett_search_type ?? "—"}</span>
        </p>
      </div>
    </section>
  );
}
