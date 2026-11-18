import {
  ArrowLeft,
  Film,
  KeyRound,
  LayoutDashboard,
  LogOut,
  Pause,
  Play,
  RefreshCw,
  Users,
} from "lucide-preact";
import type { AdminHealth } from "../../types";

interface AdminHeaderProps {
  health: AdminHealth | undefined;
  autoRefresh: boolean;
  loadingOverview: boolean;
  isUsersPage: boolean;
  userCount: number;
  onToggleAutoRefresh: () => void;
  onRefresh: () => void;
  onOpenPasswordModal: () => void;
  onNavigate: (path: string) => void;
  onLogout: () => void;
}

/** Sticky top bar and sub-navigation for the Admin Control Center. */
export function AdminHeader({
  health,
  autoRefresh,
  loadingOverview,
  isUsersPage,
  userCount,
  onToggleAutoRefresh,
  onRefresh,
  onOpenPasswordModal,
  onNavigate,
  onLogout,
}: AdminHeaderProps) {
  return (
    <header class="sticky top-0 z-20 border-b border-slate-800/80 bg-slate-950/85 backdrop-blur-md">
      <div class="mx-auto flex max-w-6xl flex-col gap-2.5 px-3.5 py-3 sm:flex-row sm:flex-wrap sm:items-center sm:justify-between sm:gap-3 sm:px-6 sm:py-3.5">
        <div class="flex min-w-0 items-center gap-2.5 sm:gap-3">
          <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-indigo-500 to-violet-600 shadow-lg shadow-indigo-500/20">
            <Film class="h-5 w-5 text-white" />
          </div>
          <div class="min-w-0">
            <h1 class="truncate text-base font-semibold text-white">Admin Control Center</h1>
            <div class="mt-0.5 flex flex-wrap items-center gap-2 text-xs">
              <span class="inline-flex items-center gap-1.5 text-slate-400">
                <span
                  class={`h-2 w-2 rounded-full ${
                    health?.qbittorrent ? "bg-emerald-400" : "bg-rose-400"
                  }`}
                />
                qBittorrent
              </span>
              <span class="text-slate-700">•</span>
              <span class="inline-flex items-center gap-1.5 text-slate-400">
                <span
                  class={`h-2 w-2 rounded-full ${health?.jackett ? "bg-emerald-400" : "bg-rose-400"}`}
                />
                Jackett
              </span>
            </div>
          </div>
        </div>

        <div class="grid w-full grid-cols-5 gap-1.5 sm:flex sm:w-auto sm:flex-wrap sm:items-center sm:gap-2">
          <button
            type="button"
            onClick={onToggleAutoRefresh}
            title="Toggle 3s live polling"
            class={`inline-flex flex-col items-center justify-center gap-1 rounded-lg border px-1 py-1.5 text-[10px] font-medium whitespace-nowrap transition sm:flex-row sm:gap-1.5 sm:px-2.5 sm:py-1.5 sm:text-xs ${
              autoRefresh
                ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-300"
                : "border-slate-800 bg-slate-900 text-slate-400 hover:text-slate-200"
            }`}
          >
            {autoRefresh ? <Pause class="h-3.5 w-3.5" /> : <Play class="h-3.5 w-3.5" />}
            <span>Live: {autoRefresh ? "On" : "Off"}</span>
          </button>

          <button
            type="button"
            onClick={onRefresh}
            class="inline-flex flex-col items-center justify-center gap-1 rounded-lg border border-slate-800 bg-slate-900 px-1 py-1.5 text-[10px] font-medium whitespace-nowrap text-slate-300 transition hover:border-slate-700 hover:text-white sm:flex-row sm:gap-1.5 sm:px-2.5 sm:py-1.5 sm:text-xs"
          >
            <RefreshCw class={`h-3.5 w-3.5 ${loadingOverview ? "animate-spin" : ""}`} />
            <span>Refresh</span>
          </button>

          <button
            type="button"
            onClick={onOpenPasswordModal}
            class="inline-flex flex-col items-center justify-center gap-1 rounded-lg border border-slate-800 bg-slate-900 px-1 py-1.5 text-[10px] font-medium whitespace-nowrap text-slate-300 transition hover:border-slate-700 hover:text-white sm:flex-row sm:gap-1.5 sm:px-2.5 sm:py-1.5 sm:text-xs"
          >
            <KeyRound class="h-3.5 w-3.5 text-indigo-400" />
            <span>Password</span>
          </button>

          <a
            href="/"
            onClick={(e) => {
              e.preventDefault();
              onNavigate("/");
            }}
            class="inline-flex flex-col items-center justify-center gap-1 rounded-lg border border-slate-800 bg-slate-900 px-1 py-1.5 text-[10px] font-medium whitespace-nowrap text-slate-300 transition hover:border-slate-700 hover:text-white sm:flex-row sm:gap-1.5 sm:px-2.5 sm:py-1.5 sm:text-xs"
          >
            <ArrowLeft class="h-3.5 w-3.5" />
            <span>Portal</span>
          </a>

          <button
            type="button"
            onClick={onLogout}
            class="inline-flex flex-col items-center justify-center gap-1 rounded-lg border border-rose-500/30 bg-rose-500/10 px-1 py-1.5 text-[10px] font-medium whitespace-nowrap text-rose-300 transition hover:bg-rose-500/20 sm:flex-row sm:gap-1.5 sm:px-2.5 sm:py-1.5 sm:text-xs"
          >
            <LogOut class="h-3.5 w-3.5" />
            <span>Logout</span>
          </button>
        </div>
      </div>

      <div class="border-t border-slate-800/70 bg-slate-950/60">
        <div class="mx-auto max-w-6xl px-3.5 py-2 sm:px-6">
          <nav
            aria-label="Admin navigation"
            class="grid grid-cols-2 gap-1 rounded-xl border border-slate-800/90 bg-slate-900/70 p-1 sm:inline-flex sm:w-auto"
          >
            <a
              href="/admin"
              onClick={(e) => {
                e.preventDefault();
                onNavigate("/admin");
              }}
              aria-current={!isUsersPage ? "page" : undefined}
              class={`inline-flex items-center justify-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium whitespace-nowrap transition ${
                !isUsersPage
                  ? "bg-indigo-600 text-white shadow-sm shadow-indigo-600/30"
                  : "text-slate-400 hover:text-white"
              }`}
            >
              <LayoutDashboard class="h-3.5 w-3.5" />
              Overview
            </a>

            <a
              href="/admin/users"
              onClick={(e) => {
                e.preventDefault();
                onNavigate("/admin/users");
              }}
              aria-current={isUsersPage ? "page" : undefined}
              class={`inline-flex items-center justify-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium whitespace-nowrap transition ${
                isUsersPage
                  ? "bg-indigo-600 text-white shadow-sm shadow-indigo-600/30"
                  : "text-slate-400 hover:text-white"
              }`}
            >
              <Users class="h-3.5 w-3.5" />
              Users
              <span
                class={`rounded-full px-1.5 py-0.5 text-[10px] font-semibold tabular-nums ${
                  isUsersPage ? "bg-white/20 text-white" : "bg-slate-800 text-slate-300"
                }`}
              >
                {userCount}
              </span>
            </a>
          </nav>
        </div>
      </div>
    </header>
  );
}
