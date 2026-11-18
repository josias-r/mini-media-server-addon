import { Check, ClipboardCopy, Eye, EyeOff, KeyRound, Link2, Trash2 } from "lucide-preact";
import type { AdminUser } from "../../types";
import { buildInviteUrl, buildManifestUrl, formatRelativeTime, maskApiKey } from "../../utils";

interface AdminUserRowProps {
  user: AdminUser;
  effectivePublicUrl: string;
  revealed: boolean;
  copiedToken: string | null;
  confirmingDelete: boolean;
  deleting: boolean;
  onToggleReveal: () => void;
  onCopy: (token: string, value: string) => void;
  onRequestDelete: () => void;
  onConfirmDelete: () => void;
  onCancelDelete: () => void;
}

/** Responsive user row (mobile card / desktop table row) with invite copy and revoke actions. */
export function AdminUserRow({
  user,
  effectivePublicUrl,
  revealed,
  copiedToken,
  confirmingDelete,
  deleting,
  onToggleReveal,
  onCopy,
  onRequestDelete,
  onConfirmDelete,
  onCancelDelete,
}: AdminUserRowProps) {
  const inviteToken = `invite-${user.api_key}`;
  const manifestToken = `manifest-${user.api_key}`;
  const keyToken = `key-${user.api_key}`;

  return (
    <tr class="flex flex-col gap-2.5 p-4 transition hover:bg-slate-800/30 sm:table-row sm:p-0">
      <td class="flex items-center justify-between gap-2 font-medium text-white sm:table-cell sm:px-4 sm:py-3.5">
        <span class="truncate">{user.username}</span>
        <span class="shrink-0 text-xs font-normal text-slate-400 sm:hidden">
          Created {formatRelativeTime(user.created_at)}
        </span>
      </td>
      <td class="block sm:table-cell sm:px-4 sm:py-3.5">
        <div class="flex items-center justify-between gap-1.5 rounded-xl border border-slate-800/80 bg-slate-950/80 px-3 py-2 sm:justify-start sm:rounded-none sm:border-0 sm:bg-transparent sm:p-0">
          <code class="min-w-0 truncate font-mono text-xs text-indigo-300 sm:rounded-md sm:bg-slate-950/80 sm:px-2 sm:py-1">
            {revealed ? user.api_key : maskApiKey(user.api_key)}
          </code>
          <button
            type="button"
            onClick={onToggleReveal}
            title={revealed ? "Hide key" : "Reveal key"}
            class="shrink-0 rounded-md p-1.5 text-slate-400 transition hover:bg-slate-800 hover:text-slate-200 sm:p-1"
          >
            {revealed ? <EyeOff class="h-3.5 w-3.5" /> : <Eye class="h-3.5 w-3.5" />}
          </button>
        </div>
      </td>
      <td class="flex items-center justify-between text-xs text-slate-300 tabular-nums sm:table-cell sm:px-4 sm:py-3.5">
        <div>
          {user.watch_count} stream{user.watch_count === 1 ? "" : "s"}
        </div>
        <div class="text-slate-500">Last: {formatRelativeTime(user.last_watched_at)}</div>
      </td>
      <td class="hidden text-xs whitespace-nowrap text-slate-400 sm:table-cell sm:px-4 sm:py-3.5">
        {formatRelativeTime(user.created_at)}
      </td>
      <td class="block pt-1 sm:table-cell sm:px-4 sm:py-3.5 sm:pt-3.5">
        <div class="flex flex-wrap items-center justify-end gap-1.5">
          <button
            type="button"
            onClick={() => onCopy(inviteToken, buildInviteUrl(effectivePublicUrl, user.api_key))}
            title="Copy 1-click user invite link"
            class="inline-flex flex-1 items-center justify-center gap-1 rounded-lg border border-slate-700 bg-slate-800/80 px-2.5 py-2 text-xs font-medium text-slate-200 transition hover:border-indigo-500/50 hover:text-white sm:flex-initial sm:py-1"
          >
            {copiedToken === inviteToken ? (
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
              onCopy(manifestToken, buildManifestUrl(effectivePublicUrl, user.api_key))
            }
            title="Copy Stremio manifest URL"
            class="inline-flex flex-1 items-center justify-center gap-1 rounded-lg border border-slate-700 bg-slate-800/80 px-2.5 py-2 text-xs font-medium text-slate-200 transition hover:border-slate-600 hover:text-white sm:flex-initial sm:py-1"
          >
            {copiedToken === manifestToken ? (
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
            onClick={() => onCopy(keyToken, user.api_key)}
            title="Copy raw API key"
            class="inline-flex items-center justify-center gap-1 rounded-lg border border-slate-700 bg-slate-800/80 p-2 text-xs font-medium text-slate-300 transition hover:border-slate-600 hover:text-white sm:px-2 sm:py-1"
          >
            {copiedToken === keyToken ? (
              <Check class="h-3.5 w-3.5 text-emerald-400 sm:h-3 sm:w-3" />
            ) : (
              <KeyRound class="h-3.5 w-3.5 text-slate-400 sm:h-3 sm:w-3" />
            )}
          </button>

          {confirmingDelete ? (
            <div class="inline-flex items-center gap-1">
              <button
                type="button"
                disabled={deleting}
                onClick={onConfirmDelete}
                class="rounded-lg bg-rose-600 px-2.5 py-2 text-xs font-semibold text-white transition hover:bg-rose-500 disabled:opacity-60 sm:py-1"
              >
                {deleting ? "Deleting..." : "Confirm"}
              </button>
              <button
                type="button"
                onClick={onCancelDelete}
                class="rounded-lg border border-slate-700 px-2.5 py-2 text-xs text-slate-300 hover:bg-slate-800 sm:px-2 sm:py-1"
              >
                Cancel
              </button>
            </div>
          ) : (
            <button
              type="button"
              onClick={onRequestDelete}
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
}
