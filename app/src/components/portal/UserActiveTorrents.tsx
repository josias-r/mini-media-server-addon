import { Download, RefreshCw } from "lucide-preact";
import type { UserTorrentStatus } from "../../types";
import {
  clampProgressPct,
  formatBytes,
  formatRelativeTime,
  formatSpeed,
  formatStateLabel,
  isDownloadingState,
} from "../../utils";
import { Badge } from "../Badge";
import { ProgressBar } from "../ProgressBar";

interface UserActiveTorrentsProps {
  torrents: UserTorrentStatus[];
  verifying: boolean;
  onRefresh: () => void;
}

/** Displays the signed-in user's active downloads and recent watched torrents. */
export function UserActiveTorrents({ torrents, verifying, onRefresh }: UserActiveTorrentsProps) {
  return (
    <section class="space-y-3.5 sm:space-y-4">
      <div class="flex items-center justify-between gap-3">
        <div class="flex min-w-0 items-center gap-2.5">
          <Download class="h-5 w-5 shrink-0 text-indigo-400" />
          <h2 class="text-base font-semibold leading-snug text-white sm:text-lg">
            My Active Downloads &amp; Recent Streams
          </h2>
        </div>
        <button
          type="button"
          onClick={onRefresh}
          class="inline-flex min-h-9 shrink-0 items-center gap-1.5 rounded-lg border border-slate-800 bg-slate-900 px-3 py-1.5 text-xs font-medium text-slate-300 transition hover:border-slate-700 hover:text-white"
        >
          <RefreshCw class={`h-3.5 w-3.5 ${verifying ? "animate-spin" : ""}`} />
          Refresh
        </button>
      </div>

      {torrents.length === 0 ? (
        <div class="rounded-2xl border border-dashed border-slate-800 bg-slate-900/40 p-6 text-center sm:p-8">
          <p class="text-sm text-slate-400">
            No recent streams yet. Pick any movie or episode inside Stremio and your downloads will
            appear here in real time.
          </p>
        </div>
      ) : (
        <div class="grid gap-3">
          {torrents.map((torrent) => {
            const pct = clampProgressPct(torrent.progress);
            const stateInfo = formatStateLabel(torrent.state);
            const downloading = isDownloadingState(torrent.state, torrent.progress);

            return (
              <div
                key={torrent.hash}
                class="rounded-xl border border-slate-800 bg-slate-900/70 p-3.5 transition hover:border-slate-700 sm:p-4"
              >
                <div class="flex flex-wrap items-start justify-between gap-2">
                  <div class="min-w-0 flex-1">
                    <p
                      class="line-clamp-2 break-all text-sm font-medium text-slate-100 sm:truncate"
                      title={torrent.name}
                    >
                      {torrent.name}
                    </p>
                    <div class="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-slate-400 tabular-nums sm:mt-1 sm:gap-x-4">
                      <span>{formatBytes(torrent.size)}</span>
                      {downloading && torrent.dlspeed > 0 && (
                        <span class="text-indigo-300">↓ {formatSpeed(torrent.dlspeed)}</span>
                      )}
                      <span>Watched {formatRelativeTime(torrent.last_watched_at)}</span>
                    </div>
                  </div>
                  <Badge tone={stateInfo.tone}>
                    {stateInfo.label} • {pct}%
                  </Badge>
                </div>

                <ProgressBar progressPct={pct} />
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}
