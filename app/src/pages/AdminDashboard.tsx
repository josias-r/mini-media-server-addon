import { useComputed, useSignal } from "@preact/signals";
import { Loader2, X } from "lucide-preact";
import { useEffect } from "preact/hooks";
import { adminLogout, checkAdminSession, fetchAdminOverview } from "../api";
import { AdminLogin } from "../components/AdminLogin";
import { PasswordModal } from "../components/PasswordModal";
import { AdminHeader } from "../components/admin/AdminHeader";
import { AdminKpiGrid } from "../components/admin/AdminKpiGrid";
import { AdminTorrentManager } from "../components/admin/AdminTorrentManager";
import type { AdminOverview, AdminStats } from "../types";
import { isDownloadingState } from "../utils";
import { AdminUsers } from "./AdminUsers";

interface AdminDashboardProps {
  pathname: string;
  onNavigate: (path: string) => void;
}

/** Authenticated Admin Control Center hosting system KPIs, torrent storage, and user management. */
export function AdminDashboard({ pathname, onNavigate }: AdminDashboardProps) {
  const isUsersPage = pathname.startsWith("/admin/users");

  const authChecked = useSignal(false);
  const authenticated = useSignal(false);

  const overview = useSignal<AdminOverview | null>(null);
  const loadingOverview = useSignal(false);
  const overviewError = useSignal<string | null>(null);
  const autoRefresh = useSignal(true);
  const passwordModalOpen = useSignal(false);

  const effectivePublicUrl = useComputed(() => {
    const configured = overview.value?.config.public_url?.trim();
    return configured || window.location.origin;
  });

  const stats = useComputed<AdminStats>(() => {
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

      <AdminHeader
        health={overview.value?.health}
        autoRefresh={autoRefresh.value}
        loadingOverview={loadingOverview.value}
        isUsersPage={isUsersPage}
        userCount={stats.value.userCount}
        onToggleAutoRefresh={() => {
          autoRefresh.value = !autoRefresh.value;
        }}
        onRefresh={() => void loadOverview()}
        onOpenPasswordModal={() => {
          passwordModalOpen.value = true;
        }}
        onNavigate={onNavigate}
        onLogout={() => void handleLogout()}
      />

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
            <AdminKpiGrid
              stats={stats.value}
              config={overview.value?.config}
              onNavigate={onNavigate}
            />
            <AdminTorrentManager
              torrents={overview.value?.torrents ?? []}
              onTorrentDeleted={() => loadOverview(true)}
              onError={(msg) => {
                overviewError.value = msg;
              }}
            />
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
