import { Captions, Check, ExternalLink } from "lucide-preact";
import { Badge } from "../Badge";

interface OnboardingStepWatchProps {
  active: boolean;
  onFinishSetup: () => void;
}

/** Step 3 onboarding card with playback tips and community subtitle addons. */
export function OnboardingStepWatch({ active, onFinishSetup }: OnboardingStepWatchProps) {
  if (!active) {
    return (
      <div class="rounded-2xl border border-slate-800/80 bg-slate-900/40 p-4 sm:p-5">
        <div class="flex items-center gap-3.5">
          <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-slate-700 bg-slate-800 text-sm font-bold text-slate-300">
            3
          </div>
          <div class="min-w-0 flex-1">
            <Badge tone="slate">Step 3 • Up Next</Badge>
            <h2 class="mt-1 text-base font-semibold text-slate-200 sm:text-lg">
              Pick Any Movie or Show &amp; Press Play
            </h2>
            <p class="mt-0.5 text-xs text-slate-400">
              Complete Step 2 above to unlock watching tips &amp; subtitle addons.
            </p>
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
          <Badge tone="indigo">Step 3 • Final Step</Badge>
          <h2 class="mt-1.5 text-lg font-bold text-white sm:text-xl">
            Pick Any Movie or Show &amp; Press Play
          </h2>
          <p class="mt-1 text-sm leading-relaxed text-slate-300">
            Open any movie or series in Stremio and choose a stream labeled{" "}
            <strong class="text-white">Mini Media Server</strong> from the list.
          </p>
          <p class="mt-2 text-xs leading-relaxed text-slate-400">
            <strong class="text-slate-300">Watching on a Smart TV?</strong> Set it up once on your
            phone or computer and sign into the same Stremio account on your TV — your addon syncs
            automatically.
          </p>
        </div>
      </div>

      {/* Additional Stremio Addons (Subtitles & More) */}
      <div class="mt-4 rounded-xl border border-slate-800 bg-slate-950/75 p-3.5 sm:mt-5 sm:p-4">
        <div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
          <div class="flex min-w-0 items-start gap-3">
            <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-indigo-500/30 bg-indigo-500/10 text-indigo-300">
              <Captions class="h-4 w-4" />
            </div>
            <div class="min-w-0">
              <h3 class="text-sm font-semibold text-white sm:text-base">
                Want Subtitles or Extra Features?
              </h3>
              <p class="mt-1 text-xs leading-relaxed text-slate-400 sm:text-sm">
                Mini Media Server is just a Stremio addon for streaming movies and series. Because
                Stremio is modular, you can freely install additional community addons alongside it
                — for example, to get automatic subtitles in any language.
              </p>
            </div>
          </div>

          <div class="flex shrink-0 flex-col gap-2 sm:items-end">
            <a
              href="https://stremio-addons.net/addons/opensubtitles-v3"
              target="_blank"
              rel="noopener noreferrer"
              class="inline-flex min-h-9 items-center justify-center gap-2 rounded-xl border border-indigo-500/30 bg-indigo-500/15 px-3.5 py-2 text-xs font-semibold text-indigo-200 transition hover:bg-indigo-500/25"
            >
              Get OpenSubtitles v3 (Subtitles)
              <ExternalLink class="h-3.5 w-3.5" />
            </a>
            <a
              href="https://stremio-addons.net/"
              target="_blank"
              rel="noopener noreferrer"
              class="inline-flex min-h-9 items-center justify-center gap-2 rounded-xl border border-slate-700 bg-slate-800/80 px-3.5 py-2 text-xs font-semibold text-slate-200 transition hover:border-slate-600 hover:bg-slate-800"
            >
              Browse More Stremio Addons
              <ExternalLink class="h-3.5 w-3.5" />
            </a>
          </div>
        </div>
      </div>

      {/* Step 3 Finish Trigger */}
      <div class="mt-5 flex flex-col gap-3 border-t border-slate-800 pt-4 sm:flex-row sm:items-center sm:justify-between">
        <p class="text-xs text-slate-400 sm:text-sm">
          All set? Hide this guide so your active downloads are front and center.
        </p>
        <button
          type="button"
          onClick={onFinishSetup}
          class="inline-flex min-h-10 shrink-0 items-center justify-center gap-2 rounded-xl bg-indigo-600 px-5 py-2.5 text-sm font-semibold text-white shadow-lg shadow-indigo-600/25 transition hover:bg-indigo-500 active:scale-[0.99]"
        >
          <Check class="h-4 w-4" />
          Got It — Finish Setup
        </button>
      </div>
    </div>
  );
}
