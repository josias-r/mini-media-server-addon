import {
  AlertTriangle,
  ArrowDown,
  ArrowRight,
  Check,
  ClipboardCopy,
  ExternalLink,
  Play,
} from "lucide-preact";
import { isIosDevice } from "../../utils";
import { Badge } from "../Badge";

interface OnboardingStepInstallProps {
  active: boolean;
  stremioLink: string;
  manifestUrl: string;
  copied: boolean;
  onCopyManifest: () => void;
  onCompleteStep: () => void;
  onReopenStep: () => void;
}

/** Step 2 onboarding card for downloading Stremio and installing the personal addon. */
export function OnboardingStepInstall({
  active,
  stremioLink,
  manifestUrl,
  copied,
  onCopyManifest,
  onCompleteStep,
  onReopenStep,
}: OnboardingStepInstallProps) {
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
            Install the Addon in Stremio
          </h2>
          <p class="mt-1 text-sm leading-relaxed text-slate-300">
            Add Mini Media Server to Stremio with one tap so movies and shows appear right inside
            your app.
          </p>
        </div>
      </div>

      {/* First-time Stremio helper + Compatible Clients jump link */}
      <div class="mt-4 rounded-xl border border-slate-800 bg-slate-950/75 p-3.5 sm:mt-5 sm:p-4">
        <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div class="min-w-0">
            <p class="text-sm font-medium text-slate-100">Don&apos;t have Stremio installed yet?</p>
            <p class="mt-0.5 text-xs leading-relaxed text-slate-400">
              Download the free Stremio app and create a free account first, then come back to this
              page and click the button below.
            </p>
          </div>
          <div class="flex shrink-0 flex-col items-stretch gap-1.5 sm:items-end">
            <a
              href="https://www.stremio.com/downloads"
              target="_blank"
              rel="noopener noreferrer"
              class="inline-flex min-h-9 shrink-0 items-center justify-center gap-1.5 rounded-lg border border-indigo-500/30 bg-indigo-500/10 px-3.5 py-2 text-xs font-semibold text-indigo-300 transition hover:bg-indigo-500/20"
            >
              Download Stremio Free
              <ExternalLink class="h-3.5 w-3.5" />
            </a>
            <a
              href="#compatible-clients"
              class="inline-flex items-center justify-center gap-1 text-[11px] font-medium text-slate-400 transition hover:text-indigo-300 sm:justify-end"
            >
              Or view other compatible apps (Nuvio, Harbor)
              <ArrowDown class="h-3 w-3" />
            </a>
          </div>
        </div>

        {isIos && (
          <div
            data-testid="ios-install-warning"
            role="alert"
            class="mt-3.5 rounded-xl border border-amber-500/35 bg-amber-500/10 p-3.5 text-amber-100"
          >
            <div class="flex items-start gap-2.5">
              <AlertTriangle class="mt-0.5 h-4 w-4 shrink-0 text-amber-400" />
              <div class="min-w-0 flex-1 space-y-1.5">
                <p class="text-xs font-semibold text-amber-200 sm:text-sm">
                  Using an iPhone or iPad? iOS App Store support is flaky
                </p>
                <p class="text-xs leading-relaxed text-amber-100/90">
                  Apple is actively removing Stremio-compatible apps from the App Store, so native
                  iOS support is unreliable. We recommend{" "}
                  <strong class="font-semibold text-white">installing Stremio as a web app</strong>{" "}
                  (open <span class="font-mono text-amber-200">web.stremio.com</span> in Safari and
                  tap <strong class="font-semibold text-white">Share → Add to Home Screen</strong>)
                  or{" "}
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

      {/* Step 2 Completion Trigger */}
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
