import {
  AlertCircle,
  Check,
  ClipboardCopy,
  KeyRound,
  Loader2,
  Play,
  Sparkles,
} from "lucide-preact";
import type { PublicInfo, VerifyKeyResponse } from "../../types";
import { ApiKeyInput, type ApiKeyInputProps } from "../ApiKeyInput";
import { Badge } from "../Badge";

interface ConnectKeySectionProps extends ApiKeyInputProps {
  info: PublicInfo | null;
  verifying: boolean;
  verification: VerifyKeyResponse | null;
  verifyError: string | null;
  manifestUrl: string;
  copied: boolean;
  onCopyManifest: () => void;
}

/** API key entry form, manifest URL preview, and initial connection instructions. */
export function ConnectKeySection({
  info,
  verifying,
  verification,
  verifyError,
  manifestUrl,
  copied,
  onCopyManifest,
  ...keyInputProps
}: ConnectKeySectionProps) {
  return (
    <>
      <section class="space-y-5 sm:space-y-6">
        <div class="text-left">
          <div class="mb-3 flex flex-wrap items-center gap-2">
            <Badge tone="indigo">
              <Sparkles class="h-3 w-3" />
              Debrid-style self-hosted streaming
            </Badge>
            {info && info.retention_days > 0 && (
              <span class="inline-flex sm:hidden">
                <Badge tone="slate">Retention: {info.retention_days}d</Badge>
              </span>
            )}
          </div>
          <h1 class="bg-gradient-to-r from-white via-slate-100 to-indigo-200 bg-clip-text text-2xl font-bold tracking-tight text-transparent sm:text-4xl">
            Connect Your Media Client
          </h1>
          <p class="mt-2 max-w-2xl text-sm leading-relaxed text-slate-400 sm:text-base">
            Enter your personal API key (or open an invite link from your admin) to generate your
            Stremio addon URL and track your active streams.
          </p>
        </div>

        <div class="rounded-2xl border border-slate-800 bg-slate-900/75 p-4 shadow-2xl shadow-black/40 backdrop-blur-sm sm:p-7">
          <div class="flex flex-wrap items-center justify-between gap-2.5 sm:gap-3">
            <label
              for="api-key-input"
              class="flex items-center gap-2 text-sm font-medium text-slate-200"
            >
              <KeyRound class="h-4 w-4 text-indigo-400" />
              Your API Key
            </label>

            <div class="flex flex-wrap items-center gap-2">
              {verifying && (
                <Badge tone="slate">
                  <Loader2 class="h-3 w-3 animate-spin" />
                  Checking...
                </Badge>
              )}
              {!verifying &&
                keyInputProps.apiKey.trim() !== "" &&
                verification &&
                !verification.valid && (
                  <Badge tone="rose">
                    <AlertCircle class="h-3 w-3" />
                    Invalid API Key
                  </Badge>
                )}
              {verifyError && <Badge tone="rose">{verifyError}</Badge>}
            </div>
          </div>

          <div class="mt-3">
            <ApiKeyInput {...keyInputProps} />
          </div>

          <div class="mt-4 rounded-xl border border-slate-800 bg-slate-950/80 p-3.5 sm:mt-5 sm:p-4">
            <div class="mb-1.5 flex flex-wrap items-center justify-between gap-1 text-[11px] font-medium uppercase tracking-wider text-slate-500 sm:text-xs">
              <span>Addon Manifest URL</span>
              <span>Replace YOUR_API_KEY above</span>
            </div>
            <code class="block break-all font-mono text-xs leading-relaxed text-indigo-300 select-all sm:text-base">
              {manifestUrl}
            </code>
          </div>

          <div class="mt-4 flex flex-col gap-2.5 sm:mt-5 sm:flex-row sm:gap-3">
            <button
              type="button"
              disabled
              class="inline-flex min-h-12 w-full flex-1 cursor-not-allowed items-center justify-center gap-2 rounded-xl border border-slate-800 bg-slate-800/50 px-5 py-3.5 text-sm font-semibold text-slate-500 sm:w-auto sm:py-3"
            >
              <Play class="h-4 w-4" />
              Enter Valid Key for 1-Click Install
            </button>

            <button
              type="button"
              onClick={onCopyManifest}
              class={`inline-flex min-h-12 w-full items-center justify-center gap-2 rounded-xl border px-5 py-3.5 text-sm font-semibold transition active:scale-[0.99] sm:w-auto sm:py-3 ${
                copied
                  ? "border-emerald-500/40 bg-emerald-500/15 text-emerald-300"
                  : "border-slate-700 bg-slate-800 text-slate-100 hover:border-slate-600 hover:bg-slate-700/80"
              }`}
            >
              {copied ? (
                <>
                  <Check class="h-4 w-4" />
                  Copied!
                </>
              ) : (
                <>
                  <ClipboardCopy class="h-4 w-4" />
                  Copy Manifest URL
                </>
              )}
            </button>
          </div>
        </div>
      </section>

      <section class="rounded-2xl border border-slate-800 bg-slate-900/50 p-4 sm:p-6">
        <h2 class="text-lg font-semibold text-white">How to Connect</h2>
        <ol class="mt-3.5 grid gap-2.5 text-sm text-slate-300 sm:mt-4 sm:grid-cols-2 sm:gap-3">
          <li class="flex gap-3 rounded-xl border border-slate-800/80 bg-slate-950/50 p-3 sm:p-3.5">
            <span class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-indigo-500/20 text-xs font-bold text-indigo-300">
              1
            </span>
            <span>Paste your API key above (or open an invite link from your server admin).</span>
          </li>
          <li class="flex gap-3 rounded-xl border border-slate-800/80 bg-slate-950/50 p-3 sm:p-3.5">
            <span class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-indigo-500/20 text-xs font-bold text-indigo-300">
              2
            </span>
            <span>
              Click <strong class="text-white">1-Click Install in Stremio</strong>, or copy the
              Manifest URL.
            </span>
          </li>
          <li class="flex gap-3 rounded-xl border border-slate-800/80 bg-slate-950/50 p-3 sm:p-3.5">
            <span class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-indigo-500/20 text-xs font-bold text-indigo-300">
              3
            </span>
            <span>
              For other clients, open <strong class="text-white">Settings → Addons</strong> and
              paste the URL ending in <code class="text-indigo-300">/manifest.json</code>.
            </span>
          </li>
          <li class="flex gap-3 rounded-xl border border-slate-800/80 bg-slate-950/50 p-3 sm:p-3.5">
            <span class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-indigo-500/20 text-xs font-bold text-indigo-300">
              4
            </span>
            <span>
              Browse any movie or series and select a Mini Media Server stream to start watching!
            </span>
          </li>
        </ol>
      </section>
    </>
  );
}
