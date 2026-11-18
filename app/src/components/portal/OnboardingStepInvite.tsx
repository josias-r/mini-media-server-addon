import { Check, KeyRound } from "lucide-preact";
import { ApiKeyEditorDrawer, type ApiKeyInputProps } from "../ApiKeyInput";
import { Badge } from "../Badge";

interface OnboardingStepInviteProps extends ApiKeyInputProps {
  showKeyEditor: boolean;
  verifying: boolean;
  onToggleKeyEditor: () => void;
}

/** Step 1 card showing the active personal invite and key management actions. */
export function OnboardingStepInvite({
  showKeyEditor,
  verifying,
  onToggleKeyEditor,
  ...keyInputProps
}: OnboardingStepInviteProps) {
  return (
    <div class="rounded-2xl border border-emerald-500/30 bg-emerald-500/5 p-4 sm:p-5">
      <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div class="flex min-w-0 items-start gap-3.5">
          <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-emerald-500/20 text-emerald-300 ring-1 ring-emerald-500/40">
            <Check class="h-5 w-5" />
          </div>
          <div class="min-w-0">
            <Badge tone="emerald">Step 1 • Completed</Badge>
            <h2 class="mt-1.5 text-base font-semibold text-white sm:text-lg">
              Personal Invite Activated
            </h2>
            <p class="mt-1 text-sm leading-relaxed text-slate-300">
              Your personal access pass is linked and saved on this device — no account setup or
              technical configuration needed.
            </p>
          </div>
        </div>

        <div class="flex shrink-0 flex-wrap items-center gap-2 pl-12 sm:pl-0">
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
        borderClass="border-emerald-500/20"
        verifying={verifying}
        {...keyInputProps}
      />
    </div>
  );
}
