import { CheckCircle2, KeyRound, RotateCcw } from "lucide-preact";
import { ApiKeyEditorDrawer, type ApiKeyInputProps } from "../ApiKeyInput";
import { Badge } from "../Badge";

interface OnboardingDoneBannerProps extends ApiKeyInputProps {
  username?: string;
  retentionDays?: number;
  showKeyEditor: boolean;
  verifying: boolean;
  onToggleKeyEditor: () => void;
  onShowSetupGuide: () => void;
}

/** Compact summary banner shown once the user finishes the onboarding guide. */
export function OnboardingDoneBanner({
  username,
  retentionDays,
  showKeyEditor,
  verifying,
  onToggleKeyEditor,
  onShowSetupGuide,
  ...keyInputProps
}: OnboardingDoneBannerProps) {
  return (
    <section class="space-y-3">
      <div class="flex flex-wrap items-center gap-2">
        <Badge tone="emerald">
          <CheckCircle2 class="h-3 w-3" />
          Verified • Welcome, {username}
        </Badge>
        {retentionDays !== undefined && retentionDays > 0 && (
          <span class="inline-flex sm:hidden">
            <Badge tone="slate">Retention: {retentionDays}d</Badge>
          </span>
        )}
      </div>

      <div class="rounded-2xl border border-slate-800 bg-slate-900/75 p-4 sm:p-5">
        <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div class="min-w-0">
            <h1 class="text-lg font-bold tracking-tight text-white sm:text-xl">
              Welcome, {username}!
            </h1>
            <p class="mt-0.5 text-sm text-slate-400">
              Setup complete — your Stremio addon is ready. Need to set up another device?
            </p>
          </div>

          <div class="flex shrink-0 flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={onShowSetupGuide}
              class="inline-flex min-h-9 items-center gap-1.5 rounded-lg border border-indigo-500/30 bg-indigo-500/10 px-3 py-1.5 text-xs font-semibold text-indigo-200 transition hover:bg-indigo-500/20"
            >
              <RotateCcw class="h-3.5 w-3.5" />
              Show Setup Guide
            </button>
            <button
              type="button"
              onClick={onToggleKeyEditor}
              class="inline-flex min-h-9 items-center gap-1.5 rounded-lg border border-slate-700/80 bg-slate-900/80 px-3 py-1.5 text-xs font-medium text-slate-300 transition hover:border-slate-600 hover:bg-slate-800 hover:text-white"
            >
              <KeyRound class="h-3.5 w-3.5 text-indigo-400" />
              {showKeyEditor ? "Hide Key" : "Change Key"}
            </button>
            <button
              type="button"
              onClick={keyInputProps.onClearKey}
              class="inline-flex min-h-9 items-center gap-1.5 rounded-lg border border-slate-800 bg-slate-950/70 px-3 py-1.5 text-xs font-medium text-slate-400 transition hover:border-rose-500/40 hover:text-rose-300"
            >
              Disconnect
            </button>
          </div>
        </div>

        <ApiKeyEditorDrawer
          open={showKeyEditor}
          borderClass="border-slate-800"
          verifying={verifying}
          {...keyInputProps}
        />
      </div>
    </section>
  );
}
