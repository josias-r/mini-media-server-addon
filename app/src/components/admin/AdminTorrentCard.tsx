import { Check, ClipboardCopy, Clock, Loader2, Trash2 } from "lucide-preact";
import type { AdminTorrent } from "../../types";
import {
  clampProgressPct,
  formatBytes,
  formatDuration,
  formatRelativeTime,
  formatSpeed,
  formatStateLabel,
  isDownloadingState,
} from "../../utils";
import { Badge } from "../Badge";
import { ProgressBar } from "../ProgressBar";

interface AdminTorrentCardProps {
  torrent: AdminTorrent;
  copied: boolean;
  confirmingDelete: boolean;
  deleting: boolean;
  onCopyHash: () => void;
  onRequestDelete: () => void;
  onConfirmDelete: () => void;
  onCancelDelete: () => void;
}

/** Single torrent status card with bandwidth metrics, info hash copy, and two-step deletion. */
export function AdminTorrentCard({
  torrent,
  copied,
  confirmingDelete,
  deleting,
  onCopyHash,
  onRequestDelete,
  onConfirmDelete,
  onCancelDelete,
}: AdminTorrentCardProps) {
  const pct = clampProgressPct(torrent.progress);
  const stateInfo = formatStateLabel(torrent.state);
  const downloading = isDownloadingState(torrent.state, torrent.progress);

  return (
    <div class="rounded-2xl border border-slate-800 bg-slate-900/75 p-3.5 transition hover:border-slate-700 sm:p-4">
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
              Size: <strong class="font-medium text-slate-200">{formatBytes(torrent.size)}</strong>
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
              onClick={onCopyHash}
              title="Copy full info hash"
              class="inline-flex items-center gap-1 rounded bg-slate-950/90 px-1.5 py-0.5 font-mono text-[11px] text-slate-400 transition hover:text-indigo-300"
            >
              {copied ? (
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
          {confirmingDelete ? (
            <div class="flex items-center gap-1.5">
              <button
                type="button"
                disabled={deleting}
                onClick={onConfirmDelete}
                class="inline-flex items-center gap-1.5 rounded-xl bg-rose-600 px-3 py-2 text-xs font-semibold text-white transition hover:bg-rose-500 disabled:opacity-60 sm:py-1.5"
              >
                {deleting && <Loader2 class="h-3.5 w-3.5 animate-spin" />}
                Confirm Delete
              </button>
              <button
                type="button"
                onClick={onCancelDelete}
                class="rounded-xl border border-slate-700 px-3 py-2 text-xs font-medium text-slate-300 hover:bg-slate-800 sm:px-2.5 sm:py-1.5"
              >
                Cancel
              </button>
            </div>
          ) : (
            <button
              type="button"
              onClick={onRequestDelete}
              class="inline-flex items-center gap-1.5 rounded-xl border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-xs font-medium text-rose-300 transition hover:bg-rose-500/20 sm:py-1.5"
            >
              <Trash2 class="h-3.5 w-3.5" />
              Delete
            </button>
          )}
        </div>
      </div>

      <ProgressBar progressPct={pct} />
    </div>
  );
}
