import { Check, Link2, X } from "lucide-preact";
import type { CreatedUserResponse } from "../../types";

interface CreatedUserBannerProps {
  user: CreatedUserResponse;
  inviteUrl: string;
  copied: boolean;
  onCopyInvite: () => void;
  onDismiss: () => void;
}

/** Confirmation banner shown immediately after creating a new user with their invite link. */
export function CreatedUserBanner({
  user,
  inviteUrl,
  copied,
  onCopyInvite,
  onDismiss,
}: CreatedUserBannerProps) {
  return (
    <div class="rounded-2xl border border-emerald-500/30 bg-emerald-500/10 p-4">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p class="text-sm font-semibold text-emerald-300">User “{user.username}” created!</p>
          <p class="mt-0.5 text-xs text-emerald-200/80">
            Share the invite link below — it fills in their API key automatically.
          </p>
        </div>
        <div class="flex w-full items-center gap-2 sm:w-auto">
          <button
            type="button"
            onClick={onCopyInvite}
            class="inline-flex flex-1 items-center justify-center gap-1.5 rounded-lg bg-emerald-600 px-3 py-2 text-xs font-semibold text-white transition hover:bg-emerald-500 sm:flex-initial sm:py-1.5"
          >
            {copied ? (
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
            onClick={onDismiss}
            class="rounded-lg p-1.5 text-emerald-300/70 hover:bg-emerald-500/20 hover:text-emerald-200 sm:p-1"
          >
            <X class="h-4 w-4" />
          </button>
        </div>
      </div>
      <div class="mt-3 rounded-lg bg-slate-950/60 px-3 py-2 font-mono text-xs break-all text-emerald-200">
        {inviteUrl}
      </div>
    </div>
  );
}
