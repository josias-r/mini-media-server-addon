import { useSignal, useComputed } from "@preact/signals";
import { useEffect } from "preact/hooks";
import {
  AlertCircle,
  Captions,
  Check,
  CheckCircle2,
  ClipboardCopy,
  Download,
  ExternalLink,
  Eye,
  EyeOff,
  Film,
  KeyRound,
  Loader2,
  Play,
  RefreshCw,
  ShieldCheck,
  Sparkles,
  Tv,
  X,
} from "lucide-preact";
import { fetchPublicInfo, verifyApiKey } from "../api";
import { Badge } from "../components/Badge";
import type { PublicInfo, VerifyKeyResponse } from "../types";
import {
  buildManifestUrl,
  buildStremioDeepLink,
  copyToClipboard,
  formatBytes,
  formatRelativeTime,
  formatSpeed,
  formatStateLabel,
  isDownloadingState,
} from "../utils";

const STORAGE_KEY = "mms_api_key";

interface UserPortalProps {
  onNavigate: (path: string) => void;
}

const CLIENTS = [
  {
    name: "Stremio",
    platforms: "Windows • macOS • Linux • Android • TV",
    description:
      "The official Stremio client with native addon support and seamless playback across desktop, mobile, and TV.",
    url: "https://www.stremio.com/downloads",
    cta: "Download Stremio",
  },
  {
    name: "Nuvio",
    platforms: "Android • Android TV • macOS • Windows • Samsung TV • LG TV",
    description:
      "A modern cross-platform Stremio-compatible media center for mobile, desktop, and smart TVs.",
    url: "https://nuvio.tv/",
    cta: "Visit Nuvio",
  },
  {
    name: "Harbor",
    platforms: "macOS • Windows • Linux",
    description:
      "An open-source, customizable desktop client built for the Stremio addon ecosystem.",
    url: "https://github.com/harborstremio/harbor",
    cta: "View on GitHub",
  },
];

export function UserPortal({ onNavigate }: UserPortalProps) {
  const info = useSignal<PublicInfo | null>(null);
  const apiKey = useSignal("");
  const showKey = useSignal(false);
  const showKeyEditor = useSignal(false);
  const verifying = useSignal(false);
  const verification = useSignal<VerifyKeyResponse | null>(null);
  const verifyError = useSignal<string | null>(null);
  const copied = useSignal(false);

  const effectivePublicUrl = useComputed(() => {
    const configured = info.value?.public_url?.trim();
    if (configured) return configured;
    return window.location.origin;
  });

  const manifestUrl = useComputed(() => buildManifestUrl(effectivePublicUrl.value, apiKey.value));

  const stremioLink = useComputed(() =>
    buildStremioDeepLink(effectivePublicUrl.value, apiKey.value),
  );

  const isValidKey = useComputed(() => Boolean(verification.value?.valid));

  const hasActiveDownloads = useComputed(() =>
    Boolean(
      verification.value?.active_torrents.some((t) => isDownloadingState(t.state, t.progress)),
    ),
  );

  async function runVerify(key: string) {
    const trimmed = key.trim();
    if (!trimmed) {
      verification.value = null;
      verifyError.value = null;
      verifying.value = false;
      return;
    }

    verifying.value = true;
    verifyError.value = null;
    try {
      const res = await verifyApiKey(trimmed);
      if (apiKey.value.trim() === trimmed) {
        verification.value = res;
        if (res.valid) {
          localStorage.setItem(STORAGE_KEY, trimmed);
        }
      }
    } catch (err) {
      verifyError.value = err instanceof Error ? err.message : "Unable to verify API key";
    } finally {
      verifying.value = false;
    }
  }

  useEffect(() => {
    void fetchPublicInfo()
      .then((res) => {
        info.value = res;
      })
      .catch(() => {
        // Fallback to window.location.origin if /api/info fails
      });

    const params = new URLSearchParams(window.location.search);
    const keyFromQuery = params.get("key")?.trim();
    const savedKey = localStorage.getItem(STORAGE_KEY)?.trim() ?? "";
    const initialKey = keyFromQuery || savedKey;

    if (initialKey) {
      apiKey.value = initialKey;
      if (keyFromQuery) {
        localStorage.setItem(STORAGE_KEY, keyFromQuery);
        const cleanUrl = `${window.location.pathname}${window.location.hash}`;
        window.history.replaceState({}, "", cleanUrl);
      }
      void runVerify(initialKey);
    }
  }, []);

  useEffect(() => {
    const trimmed = apiKey.value.trim();
    if (!trimmed) {
      verification.value = null;
      verifyError.value = null;
      return;
    }

    const timer = window.setTimeout(() => {
      void runVerify(trimmed);
    }, 300);

    return () => window.clearTimeout(timer);
  }, [apiKey.value]);

  useEffect(() => {
    if (!isValidKey.value || !hasActiveDownloads.value) return;
    const interval = window.setInterval(() => {
      void runVerify(apiKey.value);
    }, 3000);
    return () => window.clearInterval(interval);
  }, [isValidKey.value, hasActiveDownloads.value, apiKey.value]);

  async function handleCopyManifest() {
    const ok = await copyToClipboard(manifestUrl.value);
    if (ok) {
      copied.value = true;
      window.setTimeout(() => {
        copied.value = false;
      }, 2000);
    }
  }

  function handleClearKey() {
    apiKey.value = "";
    showKey.value = false;
    showKeyEditor.value = false;
    verification.value = null;
    verifyError.value = null;
    localStorage.removeItem(STORAGE_KEY);
  }

  function renderApiKeyInput() {
    return (
      <div class="relative">
        <input
          id="api-key-input"
          type={(showKey.value ? "text" : "password") as "text"}
          value={apiKey.value}
          onInput={(e) => {
            apiKey.value = (e.target as HTMLInputElement).value;
          }}
          placeholder="Paste your API key (e.g. 550e8400-e29b-41d4-a716-446655440000)"
          autocomplete="off"
          spellcheck={false}
          class="w-full rounded-xl border border-slate-700/80 bg-slate-950/90 py-3 pr-20 pl-3.5 font-mono text-base text-slate-100 placeholder-slate-500 transition outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500/20 sm:pl-4 sm:text-sm"
        />
        <div class="absolute inset-y-0 right-1.5 flex items-center gap-0.5">
          {apiKey.value && (
            <button
              type="button"
              onClick={handleClearKey}
              title="Clear saved key"
              class="flex h-9 w-9 items-center justify-center rounded-lg text-slate-400 transition hover:bg-slate-800 hover:text-slate-200"
            >
              <X class="h-4 w-4" />
            </button>
          )}
          <button
            type="button"
            onClick={() => {
              showKey.value = !showKey.value;
            }}
            title={showKey.value ? "Hide API key" : "Show API key"}
            class="flex h-9 w-9 items-center justify-center rounded-lg text-slate-400 transition hover:bg-slate-800 hover:text-slate-200"
          >
            {showKey.value ? <EyeOff class="h-4 w-4" /> : <Eye class="h-4 w-4" />}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div class="relative min-h-screen overflow-x-hidden">
      <div class="pointer-events-none fixed inset-0 -z-10 bg-[radial-gradient(ellipse_at_top,_rgba(99,102,241,0.16),transparent_55%)]" />

      <header class="sticky top-0 z-20 border-b border-slate-800/80 bg-slate-950/85 backdrop-blur-md">
        <div class="mx-auto flex max-w-5xl items-center justify-between gap-2.5 px-3.5 py-3 sm:gap-3 sm:px-6 sm:py-4">
          <div class="flex min-w-0 items-center gap-2.5 sm:gap-3">
            <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-indigo-500 to-violet-600 shadow-lg shadow-indigo-500/25">
              <Film class="h-5 w-5 text-white" />
            </div>
            <div class="min-w-0">
              <div class="flex min-w-0 items-center gap-2">
                <span class="truncate text-base font-semibold tracking-tight text-white">
                  Mini Media Server
                </span>
                {info.value?.version && (
                  <span class="shrink-0 rounded-md border border-slate-800 bg-slate-900 px-1.5 py-0.5 font-mono text-[11px] text-slate-400">
                    v{info.value.version}
                  </span>
                )}
              </div>
              <p class="truncate text-xs text-slate-400">Self-hosted Stremio Addon</p>
            </div>
          </div>

          <div class="flex shrink-0 items-center gap-2.5">
            {info.value && info.value.retention_days > 0 && (
              <span class="hidden sm:inline-flex">
                <Badge tone="slate">Retention: {info.value.retention_days}d</Badge>
              </span>
            )}
            <a
              href="/admin"
              onClick={(e) => {
                e.preventDefault();
                onNavigate("/admin");
              }}
              class="inline-flex min-h-9 shrink-0 items-center gap-1.5 rounded-lg border border-slate-800 bg-slate-900/90 px-3 py-1.5 text-xs font-medium whitespace-nowrap text-slate-300 transition hover:border-slate-700 hover:bg-slate-800 hover:text-white"
            >
              <ShieldCheck class="h-3.5 w-3.5 text-indigo-400" />
              Admin Panel
            </a>
          </div>
        </div>
      </header>

      <main class="mx-auto max-w-5xl space-y-6 px-3.5 py-6 pb-12 sm:space-y-10 sm:px-6 sm:py-10">
        {/* Hero & Onboarding / Quick Connect */}
        {isValidKey.value && verification.value ? (
          <section class="space-y-5 sm:space-y-6">
            <div class="text-left">
              <div class="mb-3 flex flex-wrap items-center gap-2">
                <Badge tone="emerald">
                  <CheckCircle2 class="h-3 w-3" />
                  Verified • Welcome, {verification.value.username}
                </Badge>
                {info.value && info.value.retention_days > 0 && (
                  <span class="inline-flex sm:hidden">
                    <Badge tone="slate">Retention: {info.value.retention_days}d</Badge>
                  </span>
                )}
              </div>
              <h1 class="bg-gradient-to-r from-white via-slate-100 to-indigo-200 bg-clip-text text-2xl font-bold tracking-tight text-transparent sm:text-4xl">
                Welcome, {verification.value.username}!
              </h1>
              <p class="mt-2 max-w-2xl text-sm leading-relaxed text-slate-400 sm:text-base">
                Your personal invite is active. Follow these 3 quick steps to add Mini Media Server
                to Stremio and start watching.
              </p>
            </div>

            <div class="space-y-4">
              {/* Step 1: Completed Invite */}
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
                        Your personal access pass is linked and saved on this device — no account
                        setup or technical configuration needed.
                      </p>
                    </div>
                  </div>

                  <div class="flex shrink-0 flex-wrap items-center gap-2 pl-12 sm:pl-0">
                    <button
                      type="button"
                      onClick={() => {
                        showKeyEditor.value = !showKeyEditor.value;
                      }}
                      class="inline-flex min-h-9 items-center gap-1.5 rounded-lg border border-slate-700/80 bg-slate-900/80 px-3 py-1.5 text-xs font-medium text-slate-300 transition hover:border-slate-600 hover:bg-slate-800 hover:text-white"
                    >
                      <KeyRound class="h-3.5 w-3.5 text-indigo-400" />
                      {showKeyEditor.value ? "Hide Key" : "Change Key"}
                    </button>
                    <button
                      type="button"
                      onClick={handleClearKey}
                      class="inline-flex min-h-9 items-center gap-1.5 rounded-lg border border-slate-800 bg-slate-950/70 px-3 py-1.5 text-xs font-medium text-slate-400 transition hover:border-rose-500/40 hover:text-rose-300"
                    >
                      Disconnect
                    </button>
                  </div>
                </div>

                {showKeyEditor.value && (
                  <div class="mt-4 border-t border-emerald-500/20 pt-4">
                    <div class="flex flex-wrap items-center justify-between gap-2">
                      <label
                        for="api-key-input"
                        class="flex items-center gap-2 text-xs font-medium text-slate-300"
                      >
                        <KeyRound class="h-3.5 w-3.5 text-indigo-400" />
                        Your Saved API Key
                      </label>
                      {verifying.value && (
                        <Badge tone="slate">
                          <Loader2 class="h-3 w-3 animate-spin" />
                          Checking...
                        </Badge>
                      )}
                    </div>
                    <div class="mt-2">{renderApiKeyInput()}</div>
                  </div>
                )}
              </div>

              {/* Step 2: Primary Install Spotlight */}
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
                      Add Mini Media Server to Stremio with one tap so movies and shows appear right
                      inside your app.
                    </p>
                  </div>
                </div>

                {/* First-time Stremio helper */}
                <div class="mt-4 flex flex-col gap-3 rounded-xl border border-slate-800 bg-slate-950/75 p-3.5 sm:mt-5 sm:flex-row sm:items-center sm:justify-between sm:p-4">
                  <div class="min-w-0">
                    <p class="text-sm font-medium text-slate-100">
                      Don&apos;t have Stremio installed yet?
                    </p>
                    <p class="mt-0.5 text-xs leading-relaxed text-slate-400">
                      Download the free Stremio app and create a free account first, then come back
                      to this page and click the button below.
                    </p>
                  </div>
                  <a
                    href="https://www.stremio.com/downloads"
                    target="_blank"
                    rel="noopener noreferrer"
                    class="inline-flex min-h-9 shrink-0 items-center justify-center gap-1.5 rounded-lg border border-indigo-500/30 bg-indigo-500/10 px-3.5 py-2 text-xs font-semibold text-indigo-300 transition hover:bg-indigo-500/20"
                  >
                    Download Stremio Free
                    <ExternalLink class="h-3.5 w-3.5" />
                  </a>
                </div>

                {/* Primary 1-Click Install CTA */}
                <div class="mt-4 sm:mt-5">
                  <a
                    href={stremioLink.value}
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
                      onClick={handleCopyManifest}
                      class={`inline-flex min-h-10 shrink-0 items-center justify-center gap-2 rounded-xl border px-4 py-2 text-xs font-semibold transition active:scale-[0.99] ${
                        copied.value
                          ? "border-emerald-500/40 bg-emerald-500/15 text-emerald-300"
                          : "border-slate-700 bg-slate-800 text-slate-100 hover:border-slate-600 hover:bg-slate-700/80"
                      }`}
                    >
                      {copied.value ? (
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
                    {manifestUrl.value}
                  </code>
                </div>
              </div>

              {/* Step 3: Start Watching */}
              <div class="rounded-2xl border border-slate-800 bg-slate-900/60 p-4 sm:p-5">
                <div class="flex items-start gap-3.5">
                  <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-slate-700 bg-slate-800 text-sm font-bold text-slate-200">
                    3
                  </div>
                  <div class="min-w-0 flex-1">
                    <Badge tone="slate">Step 3 • Start Watching</Badge>
                    <h2 class="mt-1.5 text-base font-semibold text-white sm:text-lg">
                      Pick Any Movie or Show &amp; Press Play
                    </h2>
                    <p class="mt-1 text-sm leading-relaxed text-slate-300">
                      Open any movie or series in Stremio and choose a stream labeled{" "}
                      <strong class="text-white">Mini Media Server</strong> from the list.
                    </p>
                    <p class="mt-2 text-xs leading-relaxed text-slate-400">
                      <strong class="text-slate-300">Watching on a Smart TV?</strong> Set it up once
                      on your phone or computer and sign into the same Stremio account on your TV —
                      your addon syncs automatically.
                    </p>
                  </div>
                </div>
              </div>
            </div>
          </section>
        ) : (
          <section class="space-y-5 sm:space-y-6">
            <div class="text-left">
              <div class="mb-3 flex flex-wrap items-center gap-2">
                <Badge tone="indigo">
                  <Sparkles class="h-3 w-3" />
                  Debrid-style self-hosted streaming
                </Badge>
                {info.value && info.value.retention_days > 0 && (
                  <span class="inline-flex sm:hidden">
                    <Badge tone="slate">Retention: {info.value.retention_days}d</Badge>
                  </span>
                )}
              </div>
              <h1 class="bg-gradient-to-r from-white via-slate-100 to-indigo-200 bg-clip-text text-2xl font-bold tracking-tight text-transparent sm:text-4xl">
                Connect Your Media Client
              </h1>
              <p class="mt-2 max-w-2xl text-sm leading-relaxed text-slate-400 sm:text-base">
                Enter your personal API key (or open an invite link from your admin) to generate
                your Stremio addon URL and track your active streams.
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
                  {verifying.value && (
                    <Badge tone="slate">
                      <Loader2 class="h-3 w-3 animate-spin" />
                      Checking...
                    </Badge>
                  )}
                  {!verifying.value &&
                    apiKey.value.trim() !== "" &&
                    verification.value &&
                    !verification.value.valid && (
                      <Badge tone="rose">
                        <AlertCircle class="h-3 w-3" />
                        Invalid API Key
                      </Badge>
                    )}
                  {verifyError.value && <Badge tone="rose">{verifyError.value}</Badge>}
                </div>
              </div>

              <div class="mt-3">{renderApiKeyInput()}</div>

              {/* Manifest URL Box */}
              <div class="mt-4 rounded-xl border border-slate-800 bg-slate-950/80 p-3.5 sm:mt-5 sm:p-4">
                <div class="mb-1.5 flex flex-wrap items-center justify-between gap-1 text-[11px] font-medium uppercase tracking-wider text-slate-500 sm:text-xs">
                  <span>Addon Manifest URL</span>
                  <span>Replace YOUR_API_KEY above</span>
                </div>
                <code class="block break-all font-mono text-xs leading-relaxed text-indigo-300 select-all sm:text-base">
                  {manifestUrl.value}
                </code>
              </div>

              {/* Action Buttons */}
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
                  onClick={handleCopyManifest}
                  class={`inline-flex min-h-12 w-full items-center justify-center gap-2 rounded-xl border px-5 py-3.5 text-sm font-semibold transition active:scale-[0.99] sm:w-auto sm:py-3 ${
                    copied.value
                      ? "border-emerald-500/40 bg-emerald-500/15 text-emerald-300"
                      : "border-slate-700 bg-slate-800 text-slate-100 hover:border-slate-600 hover:bg-slate-700/80"
                  }`}
                >
                  {copied.value ? (
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
        )}

        {/* User's Recent Streams & Active Downloads */}
        {isValidKey.value && verification.value && (
          <section class="space-y-3.5 sm:space-y-4">
            <div class="flex items-center justify-between gap-3">
              <div class="flex min-w-0 items-center gap-2.5">
                <Download class="h-5 w-5 shrink-0 text-indigo-400" />
                <h2 class="text-base font-semibold leading-snug text-white sm:text-lg">
                  My Active Downloads & Recent Streams
                </h2>
              </div>
              <button
                type="button"
                onClick={() => void runVerify(apiKey.value)}
                class="inline-flex min-h-9 shrink-0 items-center gap-1.5 rounded-lg border border-slate-800 bg-slate-900 px-3 py-1.5 text-xs font-medium text-slate-300 transition hover:border-slate-700 hover:text-white"
              >
                <RefreshCw class={`h-3.5 w-3.5 ${verifying.value ? "animate-spin" : ""}`} />
                Refresh
              </button>
            </div>

            {verification.value.active_torrents.length === 0 ? (
              <div class="rounded-2xl border border-dashed border-slate-800 bg-slate-900/40 p-6 text-center sm:p-8">
                <p class="text-sm text-slate-400">
                  No recent streams yet. Pick any movie or episode inside Stremio and your downloads
                  will appear here in real time.
                </p>
              </div>
            ) : (
              <div class="grid gap-3">
                {verification.value.active_torrents.map((torrent) => {
                  const pct = Math.min(100, Math.max(0, Math.round(torrent.progress * 100)));
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

                      <div class="mt-3 h-2 w-full overflow-hidden rounded-full bg-slate-800 sm:h-1.5">
                        <div
                          class={`h-full rounded-full transition-all duration-500 ${
                            pct >= 100 ? "bg-emerald-500" : "bg-indigo-500"
                          }`}
                          style={{ width: `${pct}%` }}
                        />
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </section>
        )}

        {/* How to Connect (shown before a valid key is active) */}
        {!isValidKey.value && (
          <section class="rounded-2xl border border-slate-800 bg-slate-900/50 p-4 sm:p-6">
            <h2 class="text-lg font-semibold text-white">How to Connect</h2>
            <ol class="mt-3.5 grid gap-2.5 text-sm text-slate-300 sm:mt-4 sm:grid-cols-2 sm:gap-3">
              <li class="flex gap-3 rounded-xl border border-slate-800/80 bg-slate-950/50 p-3 sm:p-3.5">
                <span class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-indigo-500/20 text-xs font-bold text-indigo-300">
                  1
                </span>
                <span>
                  Paste your API key above (or open an invite link from your server admin).
                </span>
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
                  Browse any movie or series and select a Mini Media Server stream to start
                  watching!
                </span>
              </li>
            </ol>
          </section>
        )}

        {/* Compatible Clients */}
        <section class="space-y-3.5 sm:space-y-4">
          <div class="flex items-center gap-2.5">
            <Tv class="h-5 w-5 shrink-0 text-indigo-400" />
            <h2 class="text-lg font-semibold text-white">Compatible Clients</h2>
          </div>
          <div class="grid gap-3 sm:grid-cols-3 sm:gap-4">
            {CLIENTS.map((client) => (
              <div
                key={client.name}
                class="flex flex-col justify-between rounded-2xl border border-slate-800 bg-slate-900/60 p-4 transition hover:-translate-y-0.5 hover:border-indigo-500/50 sm:p-5"
              >
                <div>
                  <h3 class="text-base font-semibold text-white">{client.name}</h3>
                  <p class="mt-1 text-xs font-medium text-indigo-300/90">{client.platforms}</p>
                  <p class="mt-2.5 text-sm leading-relaxed text-slate-400 sm:mt-3">
                    {client.description}
                  </p>
                </div>
                <a
                  href={client.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  class="mt-4 inline-flex w-full items-center justify-between rounded-xl border border-indigo-500/20 bg-indigo-500/10 px-3.5 py-2.5 text-sm font-medium text-indigo-300 transition hover:bg-indigo-500/15 sm:mt-5 sm:w-auto sm:justify-start sm:border-0 sm:bg-transparent sm:p-0 sm:text-indigo-400 sm:hover:bg-transparent sm:hover:text-indigo-300"
                >
                  {client.cta}
                  <ExternalLink class="h-3.5 w-3.5" />
                </a>
              </div>
            ))}
          </div>
        </section>

        {/* Additional Stremio Addons (Subtitles & More) */}
        <section class="rounded-2xl border border-slate-800 bg-slate-900/55 p-4 sm:p-6">
          <div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
            <div class="flex min-w-0 items-start gap-3.5">
              <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl border border-indigo-500/30 bg-indigo-500/10 text-indigo-300">
                <Captions class="h-5 w-5" />
              </div>
              <div class="min-w-0">
                <h2 class="text-base font-semibold text-white sm:text-lg">
                  Want Subtitles or Extra Features?
                </h2>
                <p class="mt-1 max-w-2xl text-sm leading-relaxed text-slate-400">
                  Mini Media Server is just a Stremio addon for streaming movies and series. Because
                  Stremio is modular, you can freely install additional community addons alongside
                  it — for example, to get automatic subtitles in any language.
                </p>
              </div>
            </div>

            <div class="flex shrink-0 flex-col gap-2 sm:flex-row sm:items-center">
              <a
                href="https://stremio-addons.net/addons/opensubtitles-v3"
                target="_blank"
                rel="noopener noreferrer"
                class="inline-flex min-h-10 items-center justify-center gap-2 rounded-xl border border-indigo-500/30 bg-indigo-500/15 px-4 py-2 text-xs font-semibold text-indigo-200 transition hover:bg-indigo-500/25"
              >
                Get OpenSubtitles v3 (Subtitles)
                <ExternalLink class="h-3.5 w-3.5" />
              </a>
              <a
                href="https://stremio-addons.net/"
                target="_blank"
                rel="noopener noreferrer"
                class="inline-flex min-h-10 items-center justify-center gap-2 rounded-xl border border-slate-700 bg-slate-800/80 px-4 py-2 text-xs font-semibold text-slate-200 transition hover:border-slate-600 hover:bg-slate-800"
              >
                Browse More Stremio Addons
                <ExternalLink class="h-3.5 w-3.5" />
              </a>
            </div>
          </div>
        </section>
      </main>
    </div>
  );
}
