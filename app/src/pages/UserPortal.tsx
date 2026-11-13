import { useSignal, useComputed } from "@preact/signals";
import { useEffect } from "preact/hooks";
import {
  AlertCircle,
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
    name: "Fusion — Media Center",
    platforms: "iOS • iPadOS • Apple TV",
    description:
      "A sleek third-party Stremio-compatible media center tailored for Apple devices with Apple TV support.",
    url: "https://apps.apple.com/us/app/fusion-media-center/id6759285919",
    cta: "View on App Store",
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
    verification.value = null;
    verifyError.value = null;
    localStorage.removeItem(STORAGE_KEY);
  }

  return (
    <div class="relative min-h-screen overflow-x-hidden">
      <div class="pointer-events-none fixed inset-0 -z-10 bg-[radial-gradient(ellipse_at_top,_rgba(99,102,241,0.16),transparent_55%)]" />

      <header class="border-b border-slate-800/80 bg-slate-950/70 backdrop-blur-md">
        <div class="mx-auto flex max-w-5xl items-center justify-between px-4 py-4 sm:px-6">
          <div class="flex items-center gap-3">
            <div class="flex h-9 w-9 items-center justify-center rounded-xl bg-gradient-to-br from-indigo-500 to-violet-600 shadow-lg shadow-indigo-500/25">
              <Film class="h-5 w-5 text-white" />
            </div>
            <div>
              <div class="flex items-center gap-2">
                <span class="text-base font-semibold tracking-tight text-white">
                  Mini Media Server
                </span>
                {info.value?.version && (
                  <span class="rounded-md border border-slate-800 bg-slate-900 px-1.5 py-0.5 font-mono text-[11px] text-slate-400">
                    v{info.value.version}
                  </span>
                )}
              </div>
              <p class="text-xs text-slate-400">Self-hosted Stremio Addon</p>
            </div>
          </div>

          <div class="flex items-center gap-2.5">
            {info.value && info.value.retention_days > 0 && (
              <Badge tone="slate" className="hidden sm:inline-flex">
                Retention: {info.value.retention_days}d
              </Badge>
            )}
            <a
              href="/admin"
              onClick={(e) => {
                e.preventDefault();
                onNavigate("/admin");
              }}
              class="inline-flex items-center gap-1.5 rounded-lg border border-slate-800 bg-slate-900/90 px-3 py-1.5 text-xs font-medium text-slate-300 transition hover:border-slate-700 hover:bg-slate-800 hover:text-white"
            >
              <ShieldCheck class="h-3.5 w-3.5 text-indigo-400" />
              Admin Panel
            </a>
          </div>
        </div>
      </header>

      <main class="mx-auto max-w-5xl space-y-10 px-4 py-10 sm:px-6">
        {/* Hero & Quick Connect */}
        <section class="space-y-6">
          <div class="text-center sm:text-left">
            <Badge tone="indigo" className="mb-3">
              <Sparkles class="h-3 w-3" />
              Debrid-style self-hosted streaming
            </Badge>
            <h1 class="bg-gradient-to-r from-white via-slate-100 to-indigo-200 bg-clip-text text-3xl font-bold tracking-tight text-transparent sm:text-4xl">
              Connect Your Media Client
            </h1>
            <p class="mt-2 max-w-2xl text-sm leading-relaxed text-slate-400 sm:text-base">
              Enter your personal API key (or open an invite link from your admin) to generate your
              Stremio addon URL and track your active streams.
            </p>
          </div>

          <div class="rounded-2xl border border-slate-800 bg-slate-900/75 p-5 shadow-2xl shadow-black/40 backdrop-blur-sm sm:p-7">
            <div class="flex flex-wrap items-center justify-between gap-3">
              <label
                for="api-key-input"
                class="flex items-center gap-2 text-sm font-medium text-slate-200"
              >
                <KeyRound class="h-4 w-4 text-indigo-400" />
                Your API Key
              </label>

              <div class="flex items-center gap-2">
                {verifying.value && (
                  <Badge tone="slate">
                    <Loader2 class="h-3 w-3 animate-spin" />
                    Checking...
                  </Badge>
                )}
                {!verifying.value && verification.value?.valid && (
                  <Badge tone="emerald">
                    <CheckCircle2 class="h-3 w-3" />
                    Verified • Welcome, {verification.value.username}
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

            <div class="relative mt-3">
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
                class="w-full rounded-xl border border-slate-700/80 bg-slate-950/90 py-3 pr-20 pl-4 font-mono text-sm text-slate-100 placeholder-slate-500 transition outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500/20"
              />
              <div class="absolute inset-y-0 right-2 flex items-center gap-1">
                {apiKey.value && (
                  <button
                    type="button"
                    onClick={handleClearKey}
                    title="Clear saved key"
                    class="rounded-lg p-1.5 text-slate-400 transition hover:bg-slate-800 hover:text-slate-200"
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
                  class="rounded-lg p-1.5 text-slate-400 transition hover:bg-slate-800 hover:text-slate-200"
                >
                  {showKey.value ? <EyeOff class="h-4 w-4" /> : <Eye class="h-4 w-4" />}
                </button>
              </div>
            </div>

            {/* Manifest URL Box */}
            <div class="mt-5 rounded-xl border border-slate-800 bg-slate-950/80 p-4">
              <div class="mb-1.5 flex items-center justify-between text-xs font-medium uppercase tracking-wider text-slate-500">
                <span>Addon Manifest URL</span>
                {isValidKey.value ? (
                  <span class="text-emerald-400">Ready to install</span>
                ) : (
                  <span>Replace YOUR_API_KEY above</span>
                )}
              </div>
              <code class="block break-all font-mono text-sm text-indigo-300 sm:text-base">
                {manifestUrl.value}
              </code>
            </div>

            {/* Action Buttons */}
            <div class="mt-5 flex flex-col gap-3 sm:flex-row">
              {isValidKey.value ? (
                <a
                  href={stremioLink.value}
                  class="inline-flex flex-1 items-center justify-center gap-2 rounded-xl bg-indigo-600 px-5 py-3 text-sm font-semibold text-white shadow-lg shadow-indigo-600/25 transition hover:bg-indigo-500 active:scale-[0.99]"
                >
                  <Play class="h-4 w-4 fill-current" />
                  1-Click Install in Stremio
                </a>
              ) : (
                <button
                  type="button"
                  disabled
                  class="inline-flex flex-1 cursor-not-allowed items-center justify-center gap-2 rounded-xl border border-slate-800 bg-slate-800/50 px-5 py-3 text-sm font-semibold text-slate-500"
                >
                  <Play class="h-4 w-4" />
                  Enter Valid Key for 1-Click Install
                </button>
              )}

              <button
                type="button"
                onClick={handleCopyManifest}
                class={`inline-flex items-center justify-center gap-2 rounded-xl border px-5 py-3 text-sm font-semibold transition active:scale-[0.99] ${
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

        {/* User's Recent Streams & Active Downloads */}
        {isValidKey.value && verification.value && (
          <section class="space-y-4">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2.5">
                <Download class="h-5 w-5 text-indigo-400" />
                <h2 class="text-lg font-semibold text-white">
                  My Active Downloads & Recent Streams
                </h2>
              </div>
              <button
                type="button"
                onClick={() => void runVerify(apiKey.value)}
                class="inline-flex items-center gap-1.5 rounded-lg border border-slate-800 bg-slate-900 px-3 py-1.5 text-xs font-medium text-slate-300 transition hover:border-slate-700 hover:text-white"
              >
                <RefreshCw class={`h-3.5 w-3.5 ${verifying.value ? "animate-spin" : ""}`} />
                Refresh
              </button>
            </div>

            {verification.value.active_torrents.length === 0 ? (
              <div class="rounded-2xl border border-dashed border-slate-800 bg-slate-900/40 p-8 text-center">
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
                      class="rounded-xl border border-slate-800 bg-slate-900/70 p-4 transition hover:border-slate-700"
                    >
                      <div class="flex flex-wrap items-start justify-between gap-2">
                        <div class="min-w-0 flex-1">
                          <p
                            class="truncate text-sm font-medium text-slate-100"
                            title={torrent.name}
                          >
                            {torrent.name}
                          </p>
                          <div class="mt-1 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-slate-400 tabular-nums">
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

                      <div class="mt-3 h-1.5 w-full overflow-hidden rounded-full bg-slate-800">
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

        {/* How to Connect */}
        <section class="rounded-2xl border border-slate-800 bg-slate-900/50 p-6">
          <h2 class="text-lg font-semibold text-white">How to Connect</h2>
          <ol class="mt-4 grid gap-3 text-sm text-slate-300 sm:grid-cols-2">
            <li class="flex gap-3 rounded-xl border border-slate-800/80 bg-slate-950/50 p-3.5">
              <span class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-indigo-500/20 text-xs font-bold text-indigo-300">
                1
              </span>
              <span>Paste your API key above (or open an invite link from your server admin).</span>
            </li>
            <li class="flex gap-3 rounded-xl border border-slate-800/80 bg-slate-950/50 p-3.5">
              <span class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-indigo-500/20 text-xs font-bold text-indigo-300">
                2
              </span>
              <span>
                Click <strong class="text-white">1-Click Install in Stremio</strong>, or copy the
                Manifest URL.
              </span>
            </li>
            <li class="flex gap-3 rounded-xl border border-slate-800/80 bg-slate-950/50 p-3.5">
              <span class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-indigo-500/20 text-xs font-bold text-indigo-300">
                3
              </span>
              <span>
                For other clients, open <strong class="text-white">Settings → Addons</strong> and
                paste the URL ending in <code class="text-indigo-300">/manifest.json</code>.
              </span>
            </li>
            <li class="flex gap-3 rounded-xl border border-slate-800/80 bg-slate-950/50 p-3.5">
              <span class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-indigo-500/20 text-xs font-bold text-indigo-300">
                4
              </span>
              <span>
                Browse any movie or series and select a Mini Media Server stream to start watching!
              </span>
            </li>
          </ol>
        </section>

        {/* Compatible Clients */}
        <section class="space-y-4">
          <div class="flex items-center gap-2.5">
            <Tv class="h-5 w-5 text-indigo-400" />
            <h2 class="text-lg font-semibold text-white">Compatible Clients</h2>
          </div>
          <div class="grid gap-4 sm:grid-cols-3">
            {CLIENTS.map((client) => (
              <div
                key={client.name}
                class="flex flex-col justify-between rounded-2xl border border-slate-800 bg-slate-900/60 p-5 transition hover:-translate-y-0.5 hover:border-indigo-500/50"
              >
                <div>
                  <h3 class="text-base font-semibold text-white">{client.name}</h3>
                  <p class="mt-1 text-xs font-medium text-indigo-300/90">{client.platforms}</p>
                  <p class="mt-3 text-sm leading-relaxed text-slate-400">{client.description}</p>
                </div>
                <a
                  href={client.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  class="mt-5 inline-flex items-center gap-1.5 text-sm font-medium text-indigo-400 transition hover:text-indigo-300"
                >
                  {client.cta}
                  <ExternalLink class="h-3.5 w-3.5" />
                </a>
              </div>
            ))}
          </div>
        </section>
      </main>
    </div>
  );
}
