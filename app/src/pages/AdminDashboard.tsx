import { useComputed, useSignal } from "@preact/signals";
import { useEffect } from "preact/hooks";
import {
  Activity,
  ArrowDown,
  ArrowLeft,
  ArrowUp,
  Check,
  ClipboardCopy,
  Clock,
  Database,
  Eye,
  EyeOff,
  Film,
  HardDrive,
  KeyRound,
  Link2,
  Loader2,
  LogOut,
  Pause,
  Play,
  RefreshCw,
  Search,
  Server,
  Trash2,
  UserPlus,
  Users,
  X,
} from "lucide-preact";
import {
  adminLogout,
  checkAdminSession,
  createUser,
  deleteTorrent,
  deleteUser,
  fetchAdminOverview,
} from "../api";
import { AdminLogin } from "../components/AdminLogin";
import { Badge } from "../components/Badge";
import { PasswordModal } from "../components/PasswordModal";
import type { AdminOverview, CreatedUserResponse } from "../types";
import {
  buildInviteUrl,
  buildManifestUrl,
  copyToClipboard,
  formatBytes,
  formatDuration,
  formatRelativeTime,
  formatSpeed,
  formatStateLabel,
  isDownloadingState,
} from "../utils";

interface AdminDashboardProps {
  onNavigate: (path: string) => void;
}

type TorrentFilter = "all" | "downloading" | "seeding" | "unwatched";
type TorrentSort = "added_desc" | "added_asc" | "size_desc" | "watched_desc" | "progress_desc";

export function AdminDashboard({ onNavigate }: AdminDashboardProps) {
  const authChecked = useSignal(false);
  const authenticated = useSignal(false);

  const overview = useSignal<AdminOverview | null>(null);
  const loadingOverview = useSignal(false);
  const overviewError = useSignal<string | null>(null);
  const autoRefresh = useSignal(true);

  const newUsername = useSignal("");
  const creatingUser = useSignal(false);
  const createUserError = useSignal<string | null>(null);
  const lastCreatedUser = useSignal<CreatedUserResponse | null>(null);

  const revealedKeys = useSignal<Record<string, boolean>>({});
  const copiedToken = useSignal<string | null>(null);
  const confirmDeleteUserKey = useSignal<string | null>(null);
  const deletingUserKey = useSignal<string | null>(null);

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
      lastCreatedUser.value = null;
    }
  }

  async function handleCreateUser(e: Event) {
    e.preventDefault();
    const trimmed = newUsername.value.trim();
    if (!trimmed) return;
    creatingUser.value = true;
    createUserError.value = null;
    try {
      const created = await createUser(trimmed);
      lastCreatedUser.value = created;
      newUsername.value = "";
      await loadOverview(true);
    } catch (err) {
      createUserError.value = err instanceof Error ? err.message : "Failed to create user";
    } finally {
      creatingUser.value = false;
    }
  }

  async function handleDeleteUser(apiKey: string) {
    deletingUserKey.value = apiKey;
    try {
      await deleteUser(apiKey);
      confirmDeleteUserKey.value = null;
      if (lastCreatedUser.value?.api_key === apiKey) {
        lastCreatedUser.value = null;
      }
      await loadOverview(true);
    } catch (err) {
      overviewError.value = err instanceof Error ? err.message : "Failed to delete user";
    } finally {
      deletingUserKey.value = null;
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

  function toggleRevealKey(apiKey: string) {
    revealedKeys.value = {
      ...revealedKeys.value,
      [apiKey]: !revealedKeys.value[apiKey],
    };
  }

  function maskKey(key: string): string {
    if (key.length <= 12) return "••••••••••••";
    return `${key.slice(0, 8)}••••••••••••${key.slice(-6)}`;
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

        {/* KPI Cards */}
        <section class="grid grid-cols-2 gap-2.5 sm:gap-4 lg:grid-cols-4">
          <div class="rounded-2xl border border-slate-800 bg-slate-900/70 p-3.5 sm:p-4">
            <div class="flex items-center justify-between text-xs font-medium text-slate-400">
              <span>Active Users</span>
              <Users class="h-4 w-4 shrink-0 text-indigo-400" />
            </div>
            <p class="mt-2 text-xl font-bold text-white tabular-nums sm:text-2xl">
              {stats.value.userCount}
            </p>
            <p class="mt-1 text-xs text-slate-400 tabular-nums">
              {stats.value.totalStreams} total stream{stats.value.totalStreams === 1 ? "" : "s"}{" "}
              recorded
            </p>
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
              Across {stats.value.torrentCount} torrent{stats.value.torrentCount === 1 ? "" : "s"}
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

        {/* Users & Invite Links */}
        <section class="space-y-3.5 sm:space-y-4">
          <div class="flex flex-wrap items-center justify-between gap-3 sm:gap-4">
            <div>
              <h2 class="text-lg font-semibold text-white">Users & Invite Links</h2>
              <p class="text-xs text-slate-400">
                Create API keys and share 1-click invite links that auto-configure the user portal.
              </p>
            </div>

            <form onSubmit={handleCreateUser} class="flex w-full gap-2 sm:w-auto">
              <input
                type="text"
                value={newUsername.value}
                onInput={(e) => {
                  newUsername.value = (e.target as HTMLInputElement).value;
                }}
                placeholder="New username..."
                required
                class="min-w-0 flex-1 rounded-xl border border-slate-700 bg-slate-900 px-3.5 py-2.5 text-base text-white placeholder-slate-500 outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500/20 sm:w-56 sm:py-2 sm:text-sm"
              />
              <button
                type="submit"
                disabled={creatingUser.value}
                class="inline-flex shrink-0 items-center gap-1.5 rounded-xl bg-indigo-600 px-4 py-2.5 text-sm font-semibold whitespace-nowrap text-white shadow-lg shadow-indigo-600/20 transition hover:bg-indigo-500 disabled:opacity-60 sm:py-2"
              >
                {creatingUser.value ? (
                  <Loader2 class="h-4 w-4 animate-spin" />
                ) : (
                  <UserPlus class="h-4 w-4" />
                )}
                Add User
              </button>
            </form>
          </div>

          {createUserError.value && (
            <div class="rounded-xl border border-rose-500/30 bg-rose-500/10 px-4 py-2.5 text-sm text-rose-300">
              {createUserError.value}
            </div>
          )}

          {lastCreatedUser.value && (
            <div class="rounded-2xl border border-emerald-500/30 bg-emerald-500/10 p-4">
              <div class="flex flex-wrap items-center justify-between gap-3">
                <div>
                  <p class="text-sm font-semibold text-emerald-300">
                    User “{lastCreatedUser.value.username}” created!
                  </p>
                  <p class="mt-0.5 text-xs text-emerald-200/80">
                    Share the invite link below — it fills in their API key automatically.
                  </p>
                </div>
                <div class="flex w-full items-center gap-2 sm:w-auto">
                  <button
                    type="button"
                    onClick={() =>
                      void triggerCopy(
                        "new-invite",
                        buildInviteUrl(
                          effectivePublicUrl.value,
                          lastCreatedUser.value?.api_key ?? "",
                        ),
                      )
                    }
                    class="inline-flex flex-1 items-center justify-center gap-1.5 rounded-lg bg-emerald-600 px-3 py-2 text-xs font-semibold text-white transition hover:bg-emerald-500 sm:flex-initial sm:py-1.5"
                  >
                    {copiedToken.value === "new-invite" ? (
                      <>
                        <Check class="h-3.5 w-3.5" />
                        Copied Invite!
                      </>
                    ) : (
                      <>
                        <Link2 class="h-3.5 w-3.5" />
                        Copy Invite Link
                      </>
                    )}
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      lastCreatedUser.value = null;
                    }}
                    class="rounded-lg p-1.5 text-emerald-300/70 hover:bg-emerald-500/20 hover:text-emerald-200 sm:p-1"
                  >
                    <X class="h-4 w-4" />
                  </button>
                </div>
              </div>
              <div class="mt-3 rounded-lg bg-slate-950/60 px-3 py-2 font-mono text-xs break-all text-emerald-200">
                {buildInviteUrl(effectivePublicUrl.value, lastCreatedUser.value.api_key)}
              </div>
            </div>
          )}

          <div class="overflow-hidden rounded-2xl border border-slate-800 bg-slate-900/70">
            {overview.value && overview.value.users.length === 0 ? (
              <div class="p-8 text-center text-sm text-slate-400">
                No users yet. Create one above to generate an API key.
              </div>
            ) : (
              <div class="sm:overflow-x-auto">
                <table class="block w-full text-left text-sm sm:table">
                  <thead class="hidden border-b border-slate-800 bg-slate-950/50 text-xs uppercase tracking-wider text-slate-400 sm:table-header-group">
                    <tr>
                      <th class="px-4 py-3 font-medium">User</th>
                      <th class="px-4 py-3 font-medium">API Key</th>
                      <th class="px-4 py-3 font-medium">Activity</th>
                      <th class="px-4 py-3 font-medium">Created</th>
                      <th class="px-4 py-3 text-right font-medium">Actions</th>
                    </tr>
                  </thead>
                  <tbody class="block divide-y divide-slate-800/80 sm:table-row-group">
                    {(overview.value?.users ?? []).map((user) => {
                      const revealed = Boolean(revealedKeys.value[user.api_key]);
                      const inviteToken = `invite-${user.api_key}`;
                      const manifestToken = `manifest-${user.api_key}`;
                      const keyToken = `key-${user.api_key}`;
                      const confirming = confirmDeleteUserKey.value === user.api_key;
                      const deleting = deletingUserKey.value === user.api_key;

                      return (
                        <tr
                          key={user.api_key}
                          class="flex flex-col gap-2.5 p-4 transition hover:bg-slate-800/30 sm:table-row sm:p-0"
                        >
                          <td class="flex items-center justify-between gap-2 font-medium text-white sm:table-cell sm:px-4 sm:py-3.5">
                            <span class="truncate">{user.username}</span>
                            <span class="shrink-0 text-xs font-normal text-slate-400 sm:hidden">
                              Created {formatRelativeTime(user.created_at)}
                            </span>
                          </td>
                          <td class="block sm:table-cell sm:px-4 sm:py-3.5">
                            <div class="flex items-center justify-between gap-1.5 rounded-xl border border-slate-800/80 bg-slate-950/80 px-3 py-2 sm:justify-start sm:rounded-none sm:border-0 sm:bg-transparent sm:p-0">
                              <code class="min-w-0 truncate font-mono text-xs text-indigo-300 sm:rounded-md sm:bg-slate-950/80 sm:px-2 sm:py-1">
                                {revealed ? user.api_key : maskKey(user.api_key)}
                              </code>
                              <button
                                type="button"
                                onClick={() => toggleRevealKey(user.api_key)}
                                title={revealed ? "Hide key" : "Reveal key"}
                                class="shrink-0 rounded-md p-1.5 text-slate-400 transition hover:bg-slate-800 hover:text-slate-200 sm:p-1"
                              >
                                {revealed ? (
                                  <EyeOff class="h-3.5 w-3.5" />
                                ) : (
                                  <Eye class="h-3.5 w-3.5" />
                                )}
                              </button>
                            </div>
                          </td>
                          <td class="flex items-center justify-between text-xs text-slate-300 tabular-nums sm:table-cell sm:px-4 sm:py-3.5">
                            <div>
                              {user.watch_count} stream{user.watch_count === 1 ? "" : "s"}
                            </div>
                            <div class="text-slate-500">
                              Last: {formatRelativeTime(user.last_watched_at)}
                            </div>
                          </td>
                          <td class="hidden text-xs whitespace-nowrap text-slate-400 sm:table-cell sm:px-4 sm:py-3.5">
                            {formatRelativeTime(user.created_at)}
                          </td>
                          <td class="block pt-1 sm:table-cell sm:px-4 sm:py-3.5 sm:pt-3.5">
                            <div class="flex flex-wrap items-center justify-end gap-1.5">
                              <button
                                type="button"
                                onClick={() =>
                                  void triggerCopy(
                                    inviteToken,
                                    buildInviteUrl(effectivePublicUrl.value, user.api_key),
                                  )
                                }
                                title="Copy 1-click user invite link"
                                class="inline-flex flex-1 items-center justify-center gap-1 rounded-lg border border-slate-700 bg-slate-800/80 px-2.5 py-2 text-xs font-medium text-slate-200 transition hover:border-indigo-500/50 hover:text-white sm:flex-initial sm:py-1"
                              >
                                {copiedToken.value === inviteToken ? (
                                  <>
                                    <Check class="h-3 w-3 text-emerald-400" />
                                    Invite
                                  </>
                                ) : (
                                  <>
                                    <Link2 class="h-3 w-3 text-indigo-400" />
                                    Invite
                                  </>
                                )}
                              </button>

                              <button
                                type="button"
                                onClick={() =>
                                  void triggerCopy(
                                    manifestToken,
                                    buildManifestUrl(effectivePublicUrl.value, user.api_key),
                                  )
                                }
                                title="Copy Stremio manifest URL"
                                class="inline-flex flex-1 items-center justify-center gap-1 rounded-lg border border-slate-700 bg-slate-800/80 px-2.5 py-2 text-xs font-medium text-slate-200 transition hover:border-slate-600 hover:text-white sm:flex-initial sm:py-1"
                              >
                                {copiedToken.value === manifestToken ? (
                                  <>
                                    <Check class="h-3 w-3 text-emerald-400" />
                                    Manifest
                                  </>
                                ) : (
                                  <>
                                    <ClipboardCopy class="h-3 w-3 text-slate-400" />
                                    Manifest
                                  </>
                                )}
                              </button>

                              <button
                                type="button"
                                onClick={() => void triggerCopy(keyToken, user.api_key)}
                                title="Copy raw API key"
                                class="inline-flex items-center justify-center gap-1 rounded-lg border border-slate-700 bg-slate-800/80 p-2 text-xs font-medium text-slate-300 transition hover:border-slate-600 hover:text-white sm:px-2 sm:py-1"
                              >
                                {copiedToken.value === keyToken ? (
                                  <Check class="h-3.5 w-3.5 text-emerald-400 sm:h-3 sm:w-3" />
                                ) : (
                                  <KeyRound class="h-3.5 w-3.5 text-slate-400 sm:h-3 sm:w-3" />
                                )}
                              </button>

                              {confirming ? (
                                <div class="inline-flex items-center gap-1">
                                  <button
                                    type="button"
                                    disabled={deleting}
                                    onClick={() => void handleDeleteUser(user.api_key)}
                                    class="rounded-lg bg-rose-600 px-2.5 py-2 text-xs font-semibold text-white transition hover:bg-rose-500 disabled:opacity-60 sm:py-1"
                                  >
                                    {deleting ? "Deleting..." : "Confirm"}
                                  </button>
                                  <button
                                    type="button"
                                    onClick={() => {
                                      confirmDeleteUserKey.value = null;
                                    }}
                                    class="rounded-lg border border-slate-700 px-2.5 py-2 text-xs text-slate-300 hover:bg-slate-800 sm:px-2 sm:py-1"
                                  >
                                    Cancel
                                  </button>
                                </div>
                              ) : (
                                <button
                                  type="button"
                                  onClick={() => {
                                    confirmDeleteUserKey.value = user.api_key;
                                  }}
                                  title="Revoke user"
                                  class="rounded-lg border border-rose-500/30 bg-rose-500/10 p-2 text-rose-300 transition hover:bg-rose-500/20 sm:p-1.5"
                                >
                                  <Trash2 class="h-3.5 w-3.5" />
                                </button>
                              )}
                            </div>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
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
                              ↓ {formatSpeed(torrent.dlspeed)} • ETA {formatDuration(torrent.eta)}
                            </span>
                          )}

                          {torrent.upspeed > 0 && (
                            <span class="text-emerald-300">↑ {formatSpeed(torrent.upspeed)}</span>
                          )}

                          <span>Added: {formatRelativeTime(torrent.added_on)}</span>
                          <span>Last Watched: {formatRelativeTime(torrent.last_watched_at)}</span>

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
