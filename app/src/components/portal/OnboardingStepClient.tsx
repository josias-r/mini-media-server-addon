import { AlertTriangle, ArrowDown, ArrowRight, Check, Download, ExternalLink } from "lucide-preact";
import { isIosDevice } from "../../utils";
import { Badge } from "../Badge";

interface OnboardingStepClientProps {
  active: boolean;
  onCompleteStep: () => void;
  onReopenStep: () => void;
}

/** Step 2 onboarding card for downloading Stremio or choosing a compatible media player. */
export function OnboardingStepClient({
  active,
  onCompleteStep,
  onReopenStep,
}: OnboardingStepClientProps) {
  if (!active) {
    return (
      <div class="rounded-2xl border border-emerald-500/30 bg-emerald-500/5 p-4 sm:p-5">
        <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div class="flex min-w-0 items-start gap-3.5">
            <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-emerald-500/20 text-emerald-300 ring-1 ring-emerald-500/40">
              <Check class="h-5 w-5" />
            </div>
            <div class="min-w-0">
              <Badge tone="emerald">Step 2 • Completed</Badge>
              <h2 class="mt-1.5 text-base font-semibold text-white sm:text-lg">
                Install Stremio or a Compatible App
              </h2>
              <p class="mt-1 text-sm leading-relaxed text-slate-300">
                Media player app installed on your device.
              </p>
            </div>
          </div>

          <div class="flex shrink-0 items-center pl-12 sm:pl-0">
            <button
              type="button"
              onClick={onReopenStep}
              class="inline-flex min-h-9 items-center gap-1.5 rounded-lg border border-slate-700/80 bg-slate-900/80 px-3 py-1.5 text-xs font-medium text-slate-300 transition hover:border-slate-600 hover:bg-slate-800 hover:text-white"
            >
              Show Download Step
            </button>
          </div>
        </div>
      </div>
    );
  }

  const isIos = isIosDevice();

  return (
    <div class="rounded-2xl border-2 border-indigo-500/50 bg-slate-900/85 p-4 shadow-2xl shadow-indigo-500/10 backdrop-blur-sm sm:p-7">
      <div class="flex items-start gap-3.5">
        <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-indigo-500 text-sm font-bold text-white shadow-lg shadow-indigo-500/30">
          2
        </div>
        <div class="min-w-0 flex-1">
          <Badge tone="indigo">Step 2 • Do This Now</Badge>
          <h2 class="mt-1.5 text-lg font-bold text-white sm:text-xl">
            Install Stremio or a Compatible App
          </h2>
          <p class="mt-1 text-sm leading-relaxed text-slate-300">
            Download the free Stremio app for your device — no account or sign-up is needed.
          </p>
        </div>
      </div>

      {isIos && (
        <div
          data-testid="ios-install-warning"
          role="alert"
          class="mt-4 rounded-xl border border-amber-500/35 bg-amber-500/10 p-3.5 text-amber-100 sm:mt-5"
        >
          <div class="flex items-start gap-2.5">
            <AlertTriangle class="mt-0.5 h-4 w-4 shrink-0 text-amber-400" />
            <div class="min-w-0 flex-1 space-y-1.5">
              <p class="text-xs font-semibold text-amber-200 sm:text-sm">
                Using an iPhone or iPad? iOS App Store support is flaky
              </p>
              <p class="text-xs leading-relaxed text-amber-100/90">
                Apple is actively removing Stremio-compatible apps from the App Store, so native iOS
                support is unreliable. We recommend{" "}
                <strong class="font-semibold text-white">installing Stremio as a web app</strong>{" "}
                (open <span class="font-mono text-amber-200">web.stremio.com</span> in Safari and
                tap <strong class="font-semibold text-white">Share → Add to Home Screen</strong>) or{" "}
                <strong class="font-semibold text-white">
                  looking online for a currently working iOS app
                </strong>
                .
              </p>
              <div class="pt-1">
                <a
                  href="https://web.stremio.com/"
                  target="_blank"
                  rel="noopener noreferrer"
                  class="inline-flex min-h-8 items-center gap-1.5 rounded-lg border border-amber-400/35 bg-amber-500/15 px-3 py-1.5 text-xs font-semibold text-amber-200 transition hover:bg-amber-500/25"
                >
                  Open Stremio Web App
                  <ExternalLink class="h-3.5 w-3.5" />
                </a>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Primary Download CTA + Compatible Clients jump link */}
      <div class="mt-4 sm:mt-5">
        <a
          href="https://www.stremio.com/downloads"
          target="_blank"
          rel="noopener noreferrer"
          class="inline-flex min-h-12 w-full items-center justify-center gap-2.5 rounded-xl bg-indigo-600 px-6 py-4 text-base font-semibold text-white shadow-lg shadow-indigo-600/30 transition hover:bg-indigo-500 active:scale-[0.99]"
        >
          <Download class="h-5 w-5" />
          Download Stremio Free
          <ExternalLink class="h-4 w-4 opacity-80" />
        </a>
        <div class="mt-2.5 flex justify-center">
          <a
            href="#compatible-clients"
            class="inline-flex items-center gap-1.5 text-xs font-medium text-slate-400 transition hover:text-indigo-300"
          >
            Or view other compatible apps (Nuvio, Harbor)
            <ArrowDown class="h-3.5 w-3.5" />
          </a>
        </div>
      </div>

      {/* Step 2 Completion Trigger */}
      <div class="mt-5 flex flex-col gap-3 border-t border-slate-800 pt-4 sm:flex-row sm:items-center sm:justify-between">
        <p class="text-xs text-slate-400 sm:text-sm">
          Already have Stremio or another compatible app installed?
        </p>
        <button
          type="button"
          onClick={onCompleteStep}
          class="inline-flex min-h-10 shrink-0 items-center justify-center gap-2 rounded-xl border border-emerald-500/40 bg-emerald-500/15 px-4 py-2 text-sm font-semibold text-emerald-200 transition hover:bg-emerald-500/25 active:scale-[0.99]"
        >
          I Have the App — Next Step
          <ArrowRight class="h-4 w-4" />
        </button>
      </div>
    </div>
  );
}
