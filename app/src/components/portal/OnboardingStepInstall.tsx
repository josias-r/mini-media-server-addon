import { ArrowRight, Check, ClipboardCopy, Play } from "lucide-preact";
import { Badge } from "../Badge";

interface OnboardingStepInstallProps {
  state: "upcoming" | "active" | "completed";
  stremioLink: string;
  manifestUrl: string;
  copied: boolean;
  onCopyManifest: () => void;
  onCompleteStep: () => void;
  onReopenStep: () => void;
}

/** Step 3 onboarding card for installing the personal addon inside a compatible client. */
export function OnboardingStepInstall({
  state,
  stremioLink,
  manifestUrl,
  copied,
  onCopyManifest,
  onCompleteStep,
  onReopenStep,
}: OnboardingStepInstallProps) {
  if (state === "upcoming") {
    return (
      <div class="rounded-2xl border border-slate-800/80 bg-slate-900/40 p-4 sm:p-5">
        <div class="flex items-center gap-3.5">
          <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-slate-700 bg-slate-800 text-sm font-bold text-slate-300">
            3
          </div>
          <div class="min-w-0 flex-1">
            <Badge tone="slate">Step 3 • Up Next</Badge>
            <h2 class="mt-1 text-base font-semibold text-slate-200 sm:text-lg">
              Install the Addon in Stremio
            </h2>
            <p class="mt-0.5 text-xs text-slate-400">
              Complete Step 2 above to unlock your 1-click addon install link.
            </p>
          </div>
        </div>
      </div>
    );
  }

  if (state === "completed") {
    return (
      <div class="rounded-2xl border border-emerald-500/30 bg-emerald-500/5 p-4 sm:p-5">
        <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div class="flex min-w-0 items-start gap-3.5">
            <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-emerald-500/20 text-emerald-300 ring-1 ring-emerald-500/40">
              <Check class="h-5 w-5" />
            </div>
            <div class="min-w-0">
              <Badge tone="emerald">Step 3 • Completed</Badge>
              <h2 class="mt-1.5 text-base font-semibold text-white sm:text-lg">
                Install the Addon in Stremio
              </h2>
              <p class="mt-1 text-sm leading-relaxed text-slate-300">
                Addon installed in your media player.
              </p>
            </div>
          </div>

          <div class="flex shrink-0 items-center pl-12 sm:pl-0">
            <button
              type="button"
              onClick={onReopenStep}
              class="inline-flex min-h-9 items-center gap-1.5 rounded-lg border border-slate-700/80 bg-slate-900/80 px-3 py-1.5 text-xs font-medium text-slate-300 transition hover:border-slate-600 hover:bg-slate-800 hover:text-white"
            >
              Show Install Steps
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div class="rounded-2xl border-2 border-indigo-500/50 bg-slate-900/85 p-4 shadow-2xl shadow-indigo-500/10 backdrop-blur-sm sm:p-7">
      <div class="flex items-start gap-3.5">
        <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-indigo-500 text-sm font-bold text-white shadow-lg shadow-indigo-500/30">
          3
        </div>
        <div class="min-w-0 flex-1">
          <Badge tone="indigo">Step 3 • Do This Now</Badge>
          <h2 class="mt-1.5 text-lg font-bold text-white sm:text-xl">
            Install the Addon in Stremio
          </h2>
          <p class="mt-1 text-sm leading-relaxed text-slate-300">
            Add Mini Media Server to Stremio with one tap so movies and shows appear right inside
            your app.
          </p>
        </div>
      </div>

      {/* Primary 1-Click Install CTA */}
      <div class="mt-4 sm:mt-5">
        <a
          href={stremioLink}
          class="inline-flex min-h-12 w-full items-center justify-center gap-2.5 rounded-xl bg-indigo-600 px-6 py-4 text-base font-semibold text-white shadow-lg shadow-indigo-600/30 transition hover:bg-indigo-500 active:scale-[0.99]"
        >
          <Play class="h-5 w-5 fill-current" />
          1-Click Install in Stremio
        </a>
        <p class="mt-2 text-center text-xs text-slate-400">
          A popup will open inside Stremio — simply click{" "}
          <strong class="text-slate-200">Install</strong> to finish.
        </p>
      </div>

      {/* Secondary manual copy option */}
      <div class="mt-5 rounded-xl border border-slate-800/90 bg-slate-950/70 p-3.5 sm:p-4">
        <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div class="min-w-0">
            <p class="text-xs font-semibold uppercase tracking-wider text-slate-400">
              Using another app or manual setup?
            </p>
            <p class="mt-0.5 text-xs text-slate-400">
              Copy your personal addon link and paste it into{" "}
              <strong class="text-slate-200">Settings → Addons</strong>.
            </p>
          </div>
          <button
            type="button"
            onClick={onCopyManifest}
            class={`inline-flex min-h-10 shrink-0 items-center justify-center gap-2 rounded-xl border px-4 py-2 text-xs font-semibold transition active:scale-[0.99] ${
              copied
                ? "border-emerald-500/40 bg-emerald-500/15 text-emerald-300"
                : "border-slate-700 bg-slate-800 text-slate-100 hover:border-slate-600 hover:bg-slate-700/80"
            }`}
          >
            {copied ? (
              <>
                <Check class="h-3.5 w-3.5" />
                Copied!
              </>
            ) : (
              <>
                <ClipboardCopy class="h-3.5 w-3.5" />
                Copy Manifest URL
              </>
            )}
          </button>
        </div>
        <code class="mt-2.5 block break-all rounded-lg border border-slate-800/80 bg-slate-900/90 px-3 py-2 font-mono text-xs leading-relaxed text-indigo-300 select-all">
          {manifestUrl}
        </code>
      </div>

      {/* Step 3 Completion Trigger */}
      <div class="mt-5 flex flex-col gap-3 border-t border-slate-800 pt-4 sm:flex-row sm:items-center sm:justify-between">
        <p class="text-xs text-slate-400 sm:text-sm">Already clicked install in your app?</p>
        <button
          type="button"
          onClick={onCompleteStep}
          class="inline-flex min-h-10 shrink-0 items-center justify-center gap-2 rounded-xl border border-emerald-500/40 bg-emerald-500/15 px-4 py-2 text-sm font-semibold text-emerald-200 transition hover:bg-emerald-500/25 active:scale-[0.99]"
        >
          I&apos;ve Installed It — Next Step
          <ArrowRight class="h-4 w-4" />
        </button>
      </div>
    </div>
  );
}
