import { useComputed, useSignal } from "@preact/signals";
import { useEffect } from "preact/hooks";
import {
  Activity,
  ArrowDown,
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  Check,
  ClipboardCopy,
  Clock,
  Database,
  Film,
  HardDrive,
  KeyRound,
  LayoutDashboard,
  Loader2,
  LogOut,
  Pause,
  Play,
  RefreshCw,
  Search,
  Server,
  Trash2,
  Users,
  X,
} from "lucide-preact";
import { adminLogout, checkAdminSession, deleteTorrent, fetchAdminOverview } from "../api";
import { AdminLogin } from "../components/AdminLogin";
import { Badge } from "../components/Badge";
import { PasswordModal } from "../components/PasswordModal";
import type { AdminOverview } from "../types";
import {
  copyToClipboard,
  formatBytes,
  formatDuration,
  formatRelativeTime,
  formatSpeed,
  formatStateLabel,
  isDownloadingState,
} from "../utils";
import { AdminUsers } from "./AdminUsers";

interface AdminDashboardProps {
  pathname: string;
  onNavigate: (path: string) => void;
}

type TorrentFilter = "all" | "downloading" | "seeding" | "unwatched";
type TorrentSort = "added_desc" | "added_asc" | "size_desc" | "watched_desc" | "progress_desc";

export function AdminDashboard({ pathname, onNavigate }: AdminDashboardProps) {
  const isUsersPage = pathname.startsWith("/admin/users");

  const authChecked = useSignal(false);
  const authenticated = useSignal(false);

  const overview = useSignal<AdminOverview | null>(null);
  const loadingOverview = useSignal(false);
  const overviewError = useSignal<string | null>(null);
  const autoRefresh = useSignal(true);

  const copiedToken = useSignal<string | null>(null);

  const torrentSearch = useSignal("");
  const torrentFilter = useSignal<TorrentFilter>("all");
  const torrentSort = useSignal<TorrentSort>("added_desc");
  const confirmDeleteHash = useSignal<string | null>(null);
  const deletingHash = useSignal<string | null>(null);

  const passwordModalOpen = useSignal(false);

  const effectivePublicUrl = useComputed(() => {
    const configured = overview.value?.config.public_url?.trim();
    return configured || window.location.origin;
  });

  const stats = useComputed(() => {
    const data = overview.value;
    if (!data) {
      return {
        userCount: 0,
        totalStreams: 0,
        totalBytes: 0,
        torrentCount: 0,
        dlSpeed: 0,
        upSpeed: 0,
        downloadingCount: 0,
      };
    }

    let totalStreams = 0;
    for (const u of data.users) {
      totalStreams += u.watch_count;
    }

    let totalBytes = 0;
    let dlSpeed = 0;
    let upSpeed = 0;
    let downloadingCount = 0;
    for (const t of data.torrents) {
      totalBytes += t.size;
      dlSpeed += t.dlspeed;
      upSpeed += t.upspeed;
      if (isDownloadingState(t.state, t.progress)) {
        downloadingCount += 1;
      }
    }

    return {
      userCount: data.users.length,
      totalStreams,
      totalBytes,
      torrentCount: data.torrents.length,
      dlSpeed,
      upSpeed,
      downloadingCount,
    };
  });

  const filteredTorrents = useComputed(() => {
    const data = overview.value;
    if (!data) return [];
    const query = torrentSearch.value.trim().toLowerCase();

    const filtered = data.torrents.filter((t) => {
      if (query && !t.name.toLowerCase().includes(query) && !t.hash.toLowerCase().includes(query)) {
        return false;
      }

      const downloading = isDownloadingState(t.state, t.progress);
      if (torrentFilter.value === "downloading" && !downloading) return false;
      if (torrentFilter.value === "seeding" && (downloading || t.progress < 0.999)) return false;
      if (torrentFilter.value === "unwatched" && t.last_watched_at !== null) return false;
      return true;
    });

    return [...filtered].sort((a, b) => {
      switch (torrentSort.value) {
        case "added_asc":
          return a.added_on - b.added_on;
        case "added_desc":
          return b.added_on - a.added_on;
        case "size_desc":
          return b.size - a.size;
        case "progress_desc":
          return b.progress - a.progress;
        case "watched_desc": {
          const ta = a.last_watched_at ? Date.parse(`${a.last_watched_at.replace(" ", "T")}Z`) : 0;
          const tb = b.last_watched_at ? Date.parse(`${b.last_watched_at.replace(" ", "T")}Z`) : 0;
          return tb - ta;
        }
      }
    });
  });

  async function loadOverview(silent = false) {
    if (!silent) loadingOverview.value = true;
    overviewError.value = null;
    try {
      const data = await fetchAdminOverview();
      overview.value = data;
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Failed to load dashboard";
      if (msg.toLowerCase().includes("unauthorized") || msg.includes("401")) {
        authenticated.value = false;
      } else {
        overviewError.value = msg;
      }
    } finally {
      loadingOverview.value = false;
    }
  }

  useEffect(() => {
    void checkAdminSession().then((ok) => {
      authenticated.value = ok;
      authChecked.value = true;
      if (ok) {
        void loadOverview();
      }
    });
  }, []);

  useEffect(() => {
    if (!authenticated.value || !autoRefresh.value) return;
    const id = window.setInterval(() => {
      void loadOverview(true);
    }, 3000);
    return () => window.clearInterval(id);
  }, [authenticated.value, autoRefresh.value]);

  async function handleLogout() {
    try {
      await adminLogout();
    } finally {
      authenticated.value = false;
      overview.value = null;
    }
  }

  async function handleDeleteTorrent(hash: string) {
    deletingHash.value = hash;
    try {
      await deleteTorrent(hash);
      confirmDeleteHash.value = null;
      await loadOverview(true);
    } catch (err) {
      overviewError.value = err instanceof Error ? err.message : "Failed to delete torrent";
    } finally {
      deletingHash.value = null;
    }
  }

  async function triggerCopy(token: string, value: string) {
    const ok = await copyToClipboard(value);
    if (ok) {
      copiedToken.value = token;
      window.setTimeout(() => {
        if (copiedToken.value === token) {
          copiedToken.value = null;
        }
      }, 2000);
    }
  }

  if (!authChecked.value) {
    return (
      <div class="flex min-h-screen items-center justify-center bg-slate-950">
        <div class="flex items-center gap-3 text-slate-400">
          <Loader2 class="h-5 w-5 animate-spin text-indigo-400" />
          <span class="text-sm">Checking admin session...</span>
        </div>
      </div>
    );
  }

  if (!authenticated.value) {
    return (
      <AdminLogin
        onSuccess={async () => {
          authenticated.value = true;
          await loadOverview();
        }}
        onNavigate={onNavigate}
      />
    );
  }

  return (
    <div class="relative min-h-screen overflow-x-hidden pb-16">
      <div class="pointer-events-none fixed inset-0 -z-10 bg-[radial-gradient(ellipse_at_top,_rgba(99,102,241,0.12),transparent_50%)]" />

      {/* Top Navigation */}
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
                      overview.value?.health.qbittorrent ? "bg-emerald-400" : "bg-rose-400"
                    }`}
                  />
                  qBittorrent
                </span>
                <span class="text-slate-700">•</span>
                <span class="inline-flex items-center gap-1.5 text-slate-400">
                  <span
                    class={`h-2 w-2 rounded-full ${
                      overview.value?.health.jackett ? "bg-emerald-400" : "bg-rose-400"
                    }`}
                  />
                  Jackett
                </span>
              </div>
            </div>
          </div>

          <div class="grid w-full grid-cols-5 gap-1.5 sm:flex sm:w-auto sm:flex-wrap sm:items-center sm:gap-2">
            <button
              type="button"
              onClick={() => {
                autoRefresh.value = !autoRefresh.value;
              }}
              title="Toggle 3s live polling"
              class={`inline-flex flex-col items-center justify-center gap-1 rounded-lg border px-1 py-1.5 text-[10px] font-medium whitespace-nowrap transition sm:flex-row sm:gap-1.5 sm:px-2.5 sm:py-1.5 sm:text-xs ${
                autoRefresh.value
                  ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-300"
                  : "border-slate-800 bg-slate-900 text-slate-400 hover:text-slate-200"
              }`}
            >
              {autoRefresh.value ? <Pause class="h-3.5 w-3.5" /> : <Play class="h-3.5 w-3.5" />}
              <span>Live: {autoRefresh.value ? "On" : "Off"}</span>
            </button>

            <button
              type="button"
              onClick={() => void loadOverview()}
              class="inline-flex flex-col items-center justify-center gap-1 rounded-lg border border-slate-800 bg-slate-900 px-1 py-1.5 text-[10px] font-medium whitespace-nowrap text-slate-300 transition hover:border-slate-700 hover:text-white sm:flex-row sm:gap-1.5 sm:px-2.5 sm:py-1.5 sm:text-xs"
            >
              <RefreshCw class={`h-3.5 w-3.5 ${loadingOverview.value ? "animate-spin" : ""}`} />
              <span>Refresh</span>
            </button>

            <button
              type="button"
              onClick={() => {
                passwordModalOpen.value = true;
              }}
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
              onClick={handleLogout}
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
                  {stats.value.userCount}
                </span>
              </a>
            </nav>
          </div>
        </div>
      </header>

      <main class="mx-auto max-w-6xl space-y-6 px-3.5 pt-6 sm:space-y-8 sm:px-6 sm:pt-8">
        {overviewError.value && (
          <div class="flex items-center justify-between gap-2 rounded-xl border border-rose-500/30 bg-rose-500/10 px-4 py-3 text-sm text-rose-300">
            <span class="min-w-0 break-words">{overviewError.value}</span>
            <button
              type="button"
              onClick={() => {
                overviewError.value = null;
              }}
              class="shrink-0 text-rose-300 hover:text-white"
            >
              <X class="h-4 w-4" />
            </button>
          </div>
        )}

        {isUsersPage ? (
          <AdminUsers
            users={overview.value?.users ?? []}
            effectivePublicUrl={effectivePublicUrl.value}
            onUserMutated={() => loadOverview(true)}
            onError={(msg) => {
              overviewError.value = msg;
            }}
          />
        ) : (
          <>
            {/* KPI Cards */}
            <section class="grid grid-cols-2 gap-2.5 sm:gap-4 lg:grid-cols-4">
              <div class="flex flex-col justify-between rounded-2xl border border-slate-800 bg-slate-900/70 p-3.5 sm:p-4">
                <div>
                  <div class="flex items-center justify-between text-xs font-medium text-slate-400">
                    <span>Active Users</span>
                    <Users class="h-4 w-4 shrink-0 text-indigo-400" />
                  </div>
                  <p class="mt-2 text-xl font-bold text-white tabular-nums sm:text-2xl">
                    {stats.value.userCount}
                  </p>
                  <p class="mt-1 text-xs text-slate-400 tabular-nums">
                    {stats.value.totalStreams} total stream
                    {stats.value.totalStreams === 1 ? "" : "s"} recorded
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
                  {formatBytes(stats.value.totalBytes)}
                </p>
                <p class="mt-1 text-xs text-slate-400 tabular-nums">
                  Across {stats.value.torrentCount} torrent
                  {stats.value.torrentCount === 1 ? "" : "s"}
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
                    {formatSpeed(stats.value.dlSpeed)}
                  </span>
                  <span class="inline-flex items-center gap-1 text-xs font-medium text-slate-400 sm:text-sm">
                    <ArrowUp class="h-3 w-3 sm:h-3.5 sm:w-3.5" />
                    {formatSpeed(stats.value.upSpeed)}
                  </span>
                </div>
                <p class="mt-1 text-xs text-slate-400 tabular-nums">
                  {stats.value.downloadingCount} active download
                  {stats.value.downloadingCount === 1 ? "" : "s"}
                </p>
              </div>

              <div class="rounded-2xl border border-slate-800 bg-slate-900/70 p-3.5 sm:p-4">
                <div class="flex items-center justify-between text-xs font-medium text-slate-400">
                  <span>Policy & Indexer</span>
                  <Server class="h-4 w-4 shrink-0 text-amber-400" />
                </div>
                <p class="mt-2 text-xl font-bold text-white tabular-nums sm:text-2xl">
                  {overview.value?.config.retention_days
                    ? `${overview.value.config.retention_days} Days`
                    : "Unlimited"}
                </p>
                <p class="mt-1 text-xs text-slate-400">
                  Retention • Search:{" "}
                  <span class="font-mono text-slate-300">
                    {overview.value?.config.jackett_search_type ?? "—"}
                  </span>
                </p>
              </div>
            </section>

            {/* Torrents & Storage Manager */}
            <section class="space-y-3.5 sm:space-y-4">
              <div class="flex flex-wrap items-center justify-between gap-3">
                <div class="flex items-center gap-2">
                  <Database class="h-5 w-5 shrink-0 text-indigo-400" />
                  <div>
                    <h2 class="text-lg font-semibold text-white">Torrents & Storage Manager</h2>
                    <p class="text-xs text-slate-400">
                      Monitor active downloads, watch timestamps, and reclaim disk space.
                    </p>
                  </div>
                </div>
              </div>

              {/* Search, Filter Tabs, Sort */}
              <div class="flex flex-col gap-2.5 rounded-2xl border border-slate-800 bg-slate-900/60 p-3 sm:flex-row sm:items-center sm:justify-between sm:gap-3 sm:p-3.5">
                <div class="relative flex-1">
                  <Search class="pointer-events-none absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-slate-500" />
                  <input
                    type="text"
                    value={torrentSearch.value}
                    onInput={(e) => {
                      torrentSearch.value = (e.target as HTMLInputElement).value;
                    }}
                    placeholder="Filter by torrent name or hash..."
                    class="w-full rounded-xl border border-slate-700/80 bg-slate-950 py-2.5 pr-3 pl-9 text-base text-white placeholder-slate-500 outline-none focus:border-indigo-500 sm:py-2 sm:text-sm"
                  />
                </div>

                <div class="flex flex-col gap-2 sm:flex-row sm:flex-wrap sm:items-center">
                  <div class="grid w-full grid-cols-4 gap-1 rounded-xl border border-slate-800 bg-slate-950 p-1 text-xs sm:flex sm:w-auto">
                    {(
                      [
                        ["all", "All"],
                        ["downloading", "Downloading"],
                        ["seeding", "Ready"],
                        ["unwatched", "Never Watched"],
                      ] as const
                    ).map(([value, label]) => (
                      <button
                        key={value}
                        type="button"
                        onClick={() => {
                          torrentFilter.value = value;
                        }}
                        class={`truncate rounded-lg px-1 py-1.5 text-center text-[11px] font-medium transition sm:px-2.5 sm:py-1 sm:text-xs ${
                          torrentFilter.value === value
                            ? "bg-indigo-600 text-white"
                            : "text-slate-400 hover:text-slate-200"
                        }`}
                      >
                        {label}
                      </button>
                    ))}
                  </div>

                  <select
                    value={torrentSort.value}
                    onChange={(e) => {
                      torrentSort.value = (e.target as HTMLSelectElement).value as TorrentSort;
                    }}
                    class="w-full rounded-xl border border-slate-700/80 bg-slate-950 px-3 py-2.5 text-base font-medium text-slate-200 outline-none focus:border-indigo-500 sm:w-auto sm:py-2 sm:text-xs"
                  >
                    <option value="added_desc">Added (Newest)</option>
                    <option value="added_asc">Added (Oldest)</option>
                    <option value="size_desc">Size (Largest)</option>
                    <option value="watched_desc">Recently Watched</option>
                    <option value="progress_desc">Progress (Highest)</option>
                  </select>
                </div>
              </div>

              {filteredTorrents.value.length === 0 ? (
                <div class="rounded-2xl border border-dashed border-slate-800 bg-slate-900/40 p-8 text-center text-sm text-slate-400 sm:p-10">
                  {overview.value?.torrents.length === 0
                    ? "No torrents currently managed by qBittorrent."
                    : "No torrents match the current filter."}
                </div>
              ) : (
                <div class="grid gap-3">
                  {filteredTorrents.value.map((torrent) => {
                    const pct = Math.min(100, Math.max(0, Math.round(torrent.progress * 100)));
                    const stateInfo = formatStateLabel(torrent.state);
                    const downloading = isDownloadingState(torrent.state, torrent.progress);
                    const hashToken = `hash-${torrent.hash}`;
                    const confirming = confirmDeleteHash.value === torrent.hash;
                    const deleting = deletingHash.value === torrent.hash;

                    return (
                      <div
                        key={torrent.hash}
                        class="rounded-2xl border border-slate-800 bg-slate-900/75 p-3.5 transition hover:border-slate-700 sm:p-4"
                      >
                        <div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
                          <div class="min-w-0 flex-1 space-y-2">
                            <div class="flex flex-wrap items-start justify-between gap-2 sm:items-center sm:justify-start">
                              <h3
                                class="line-clamp-2 min-w-0 break-all text-sm font-semibold text-white sm:truncate sm:text-base"
                                title={torrent.name}
                              >
                                {torrent.name}
                              </h3>
                              <Badge tone={stateInfo.tone}>
                                {stateInfo.label} • {pct}%
                              </Badge>
                            </div>

                            <div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-slate-400 tabular-nums sm:gap-x-4">
                              <span>
                                Size:{" "}
                                <strong class="font-medium text-slate-200">
                                  {formatBytes(torrent.size)}
                                </strong>
                              </span>

                              {downloading && (
                                <span class="text-indigo-300">
                                  ↓ {formatSpeed(torrent.dlspeed)} • ETA{" "}
                                  {formatDuration(torrent.eta)}
                                </span>
                              )}

                              {torrent.upspeed > 0 && (
                                <span class="text-emerald-300">
                                  ↑ {formatSpeed(torrent.upspeed)}
                                </span>
                              )}

                              <span>Added: {formatRelativeTime(torrent.added_on)}</span>
                              <span>
                                Last Watched: {formatRelativeTime(torrent.last_watched_at)}
                              </span>

                              {torrent.expires_at && (
                                <span class="inline-flex items-center gap-1 text-amber-300">
                                  <Clock class="h-3 w-3" />
                                  Expires {formatRelativeTime(torrent.expires_at)}
                                </span>
                              )}

                              <button
                                type="button"
                                onClick={() => void triggerCopy(hashToken, torrent.hash)}
                                title="Copy full info hash"
                                class="inline-flex items-center gap-1 rounded bg-slate-950/90 px-1.5 py-0.5 font-mono text-[11px] text-slate-400 transition hover:text-indigo-300"
                              >
                                {copiedToken.value === hashToken ? (
                                  <>
                                    <Check class="h-3 w-3 text-emerald-400" />
                                    Copied hash
                                  </>
                                ) : (
                                  <>
                                    {torrent.hash.slice(0, 10)}…
                                    <ClipboardCopy class="h-2.5 w-2.5" />
                                  </>
                                )}
                              </button>
                            </div>
                          </div>

                          <div class="flex shrink-0 items-center justify-end gap-2 border-t border-slate-800/70 pt-2.5 sm:border-0 sm:pt-0">
                            {confirming ? (
                              <div class="flex items-center gap-1.5">
                                <button
                                  type="button"
                                  disabled={deleting}
                                  onClick={() => void handleDeleteTorrent(torrent.hash)}
                                  class="inline-flex items-center gap-1.5 rounded-xl bg-rose-600 px-3 py-2 text-xs font-semibold text-white transition hover:bg-rose-500 disabled:opacity-60 sm:py-1.5"
                                >
                                  {deleting && <Loader2 class="h-3.5 w-3.5 animate-spin" />}
                                  Confirm Delete
                                </button>
                                <button
                                  type="button"
                                  onClick={() => {
                                    confirmDeleteHash.value = null;
                                  }}
                                  class="rounded-xl border border-slate-700 px-3 py-2 text-xs font-medium text-slate-300 hover:bg-slate-800 sm:px-2.5 sm:py-1.5"
                                >
                                  Cancel
                                </button>
                              </div>
                            ) : (
                              <button
                                type="button"
                                onClick={() => {
                                  confirmDeleteHash.value = torrent.hash;
                                }}
                                class="inline-flex items-center gap-1.5 rounded-xl border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-xs font-medium text-rose-300 transition hover:bg-rose-500/20 sm:py-1.5"
                              >
                                <Trash2 class="h-3.5 w-3.5" />
                                Delete
                              </button>
                            )}
                          </div>
                        </div>

                        <div class="mt-3 h-2 w-full overflow-hidden rounded-full bg-slate-800 sm:h-1.5">
                          <div
                            class={`h-full rounded-full transition-all duration-500 ${
                              pct >= 100 ? "bg-emerald-500" : "bg-indigo-500"
                            }`}
                            style={{ width: `${pct}%` }}
                          />
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </section>
          </>
        )}
      </main>

      {passwordModalOpen.value && (
        <PasswordModal
          onClose={() => {
            passwordModalOpen.value = false;
          }}
        />
      )}
    </div>
  );
}
