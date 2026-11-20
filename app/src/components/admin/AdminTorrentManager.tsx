import { useSignal } from "@preact/signals";
import { Database, Search } from "lucide-preact";
import { deleteTorrent } from "../../api";
import type { AdminTorrent, TorrentFilter, TorrentSort } from "../../types";
import { copyToClipboard, isDownloadingState } from "../../utils";
import { AdminTorrentCard } from "./AdminTorrentCard";

interface AdminTorrentManagerProps {
  torrents: AdminTorrent[];
  onTorrentDeleted: () => Promise<void>;
  onError: (message: string) => void;
}

const FILTER_TABS: ReadonlyArray<readonly [TorrentFilter, string]> = [
  ["all", "All"],
  ["downloading", "Downloading"],
  ["seeding", "Ready"],
  ["unwatched", "Never Watched"],
];

function filterAndSortTorrents(
  torrents: AdminTorrent[],
  search: string,
  filter: TorrentFilter,
  sort: TorrentSort,
): AdminTorrent[] {
  const query = search.trim().toLowerCase();

  const filtered = torrents.filter((t) => {
    if (query && !t.name.toLowerCase().includes(query) && !t.hash.toLowerCase().includes(query)) {
      return false;
    }

    const downloading = isDownloadingState(t.state, t.progress);
    if (filter === "downloading" && !downloading) return false;
    if (filter === "seeding" && (downloading || t.progress < 0.999)) return false;
    if (filter === "unwatched" && t.last_watched_at !== null) return false;
    return true;
  });

  return [...filtered].sort((a, b) => {
    switch (sort) {
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
}

/** Searchable, filterable, and sortable list of managed qBittorrent items. */
export function AdminTorrentManager({
  torrents,
  onTorrentDeleted,
  onError,
}: AdminTorrentManagerProps) {
  const torrentSearch = useSignal("");
  const torrentFilter = useSignal<TorrentFilter>("all");
  const torrentSort = useSignal<TorrentSort>("added_desc");
  const confirmDeleteHash = useSignal<string | null>(null);
  const deletingHash = useSignal<string | null>(null);
  const copiedHash = useSignal<string | null>(null);

  const filteredTorrents = filterAndSortTorrents(
    torrents,
    torrentSearch.value,
    torrentFilter.value,
    torrentSort.value,
  );

  async function handleDeleteTorrent(hash: string) {
    deletingHash.value = hash;
    try {
      await deleteTorrent(hash);
      confirmDeleteHash.value = null;
      await onTorrentDeleted();
    } catch (err) {
      onError(err instanceof Error ? err.message : "Failed to delete torrent");
    } finally {
      deletingHash.value = null;
    }
  }

  async function handleCopyHash(hash: string) {
    const ok = await copyToClipboard(hash);
    if (ok) {
      copiedHash.value = hash;
      window.setTimeout(() => {
        if (copiedHash.value === hash) {
          copiedHash.value = null;
        }
      }, 2000);
    }
  }

  return (
    <section class="space-y-3.5 sm:space-y-4">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div class="flex items-center gap-2">
          <Database class="h-5 w-5 shrink-0 text-indigo-400" />
          <div>
            <h2 class="text-lg font-semibold text-white">Torrents &amp; Storage Manager</h2>
            <p class="text-xs text-slate-400">
              Monitor active downloads, watch timestamps, and reclaim disk space.
            </p>
          </div>
        </div>
      </div>

      {/* Search, Filter Tabs, Sort */}
      <div class="flex flex-col gap-2.5 rounded-2xl border border-slate-800 bg-slate-900/60 p-3 sm:flex-row sm:items-center sm:justify-between sm:gap-3 sm:p-3.5">
        <div class="relative min-w-0 flex-1">
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
            {FILTER_TABS.map(([value, label]) => (
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

      {filteredTorrents.length === 0 ? (
        <div class="rounded-2xl border border-dashed border-slate-800 bg-slate-900/40 p-8 text-center text-sm text-slate-400 sm:p-10">
          {torrents.length === 0
            ? "No torrents currently managed by qBittorrent."
            : "No torrents match the current filter."}
        </div>
      ) : (
        <div class="grid grid-cols-1 gap-3">
          {filteredTorrents.map((torrent) => (
            <AdminTorrentCard
              key={torrent.hash}
              torrent={torrent}
              copied={copiedHash.value === torrent.hash}
              confirmingDelete={confirmDeleteHash.value === torrent.hash}
              deleting={deletingHash.value === torrent.hash}
              onCopyHash={() => void handleCopyHash(torrent.hash)}
              onRequestDelete={() => {
                confirmDeleteHash.value = torrent.hash;
              }}
              onConfirmDelete={() => void handleDeleteTorrent(torrent.hash)}
              onCancelDelete={() => {
                confirmDeleteHash.value = null;
              }}
            />
          ))}
        </div>
      )}
    </section>
  );
}
