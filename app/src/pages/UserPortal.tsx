import { useComputed, useSignal } from "@preact/signals";
import { CheckCircle2 } from "lucide-preact";
import { useEffect } from "preact/hooks";
import { fetchPublicInfo, verifyApiKey } from "../api";
import { Badge } from "../components/Badge";
import { CompatibleClients } from "../components/portal/CompatibleClients";
import { ConnectKeySection } from "../components/portal/ConnectKeySection";
import { OnboardingDoneBanner } from "../components/portal/OnboardingDoneBanner";
import { OnboardingStepInstall } from "../components/portal/OnboardingStepInstall";
import { OnboardingStepInvite } from "../components/portal/OnboardingStepInvite";
import { OnboardingStepWatch } from "../components/portal/OnboardingStepWatch";
import { UserActiveTorrents } from "../components/portal/UserActiveTorrents";
import { UserHeader } from "../components/portal/UserHeader";
import type { OnboardingStage, PublicInfo, VerifyKeyResponse } from "../types";
import {
  buildManifestUrl,
  buildStremioDeepLink,
  copyToClipboard,
  isDownloadingState,
} from "../utils";

const STORAGE_KEY = "mms_api_key";
const ONBOARDING_STAGE_KEY = "mms_onboarding_stage";

interface UserPortalProps {
  onNavigate: (path: string) => void;
}

/** Public portal for entering/verifying an API key, installing the Stremio addon, and tracking streams. */
export function UserPortal({ onNavigate }: UserPortalProps) {
  const info = useSignal<PublicInfo | null>(null);
  const apiKey = useSignal("");
  const showKey = useSignal(false);
  const showKeyEditor = useSignal(false);
  const onboardingStage = useSignal<OnboardingStage>("install");
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

  function updateOnboardingStage(stage: OnboardingStage) {
    onboardingStage.value = stage;
    localStorage.setItem(ONBOARDING_STAGE_KEY, stage);
  }

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
    const savedStage = localStorage.getItem(ONBOARDING_STAGE_KEY);

    if (savedStage === "install" || savedStage === "watch" || savedStage === "done") {
      onboardingStage.value = savedStage;
    }

    const initialKey = keyFromQuery || savedKey;

    if (initialKey) {
      apiKey.value = initialKey;
      if (keyFromQuery) {
        if (keyFromQuery !== savedKey) {
          updateOnboardingStage("install");
        }
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
    onboardingStage.value = "install";
    verification.value = null;
    verifyError.value = null;
    localStorage.removeItem(STORAGE_KEY);
    localStorage.removeItem(ONBOARDING_STAGE_KEY);
  }

  const keyInputProps = {
    apiKey: apiKey.value,
    showKey: showKey.value,
    onApiKeyChange: (nextKey: string) => {
      apiKey.value = nextKey;
    },
    onToggleShowKey: () => {
      showKey.value = !showKey.value;
    },
    onClearKey: handleClearKey,
  };

  return (
    <div class="relative min-h-screen overflow-x-hidden">
      <div class="pointer-events-none fixed inset-0 -z-10 bg-[radial-gradient(ellipse_at_top,_rgba(99,102,241,0.16),transparent_55%)]" />

      <UserHeader info={info.value} onNavigate={onNavigate} />

      <main class="mx-auto max-w-5xl space-y-6 px-3.5 py-6 pb-12 sm:space-y-10 sm:px-6 sm:py-10">
        {isValidKey.value && verification.value ? (
          onboardingStage.value === "done" ? (
            <OnboardingDoneBanner
              username={verification.value.username}
              retentionDays={info.value?.retention_days}
              showKeyEditor={showKeyEditor.value}
              verifying={verifying.value}
              onToggleKeyEditor={() => {
                showKeyEditor.value = !showKeyEditor.value;
              }}
              onShowSetupGuide={() => updateOnboardingStage("install")}
              {...keyInputProps}
            />
          ) : (
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
                  Your personal invite is active. Follow these 3 quick steps to add Mini Media
                  Server to Stremio and start watching.
                </p>
              </div>

              <div class="space-y-4">
                <OnboardingStepInvite
                  showKeyEditor={showKeyEditor.value}
                  verifying={verifying.value}
                  onToggleKeyEditor={() => {
                    showKeyEditor.value = !showKeyEditor.value;
                  }}
                  {...keyInputProps}
                />

                <OnboardingStepInstall
                  active={onboardingStage.value === "install"}
                  stremioLink={stremioLink.value}
                  manifestUrl={manifestUrl.value}
                  copied={copied.value}
                  onCopyManifest={() => void handleCopyManifest()}
                  onCompleteStep={() => updateOnboardingStage("watch")}
                  onReopenStep={() => updateOnboardingStage("install")}
                />

                <OnboardingStepWatch
                  active={onboardingStage.value !== "install"}
                  onFinishSetup={() => updateOnboardingStage("done")}
                />
              </div>
            </section>
          )
        ) : (
          <ConnectKeySection
            info={info.value}
            verifying={verifying.value}
            verification={verification.value}
            verifyError={verifyError.value}
            manifestUrl={manifestUrl.value}
            copied={copied.value}
            onCopyManifest={() => void handleCopyManifest()}
            {...keyInputProps}
          />
        )}

        {isValidKey.value && verification.value && (
          <UserActiveTorrents
            torrents={verification.value.active_torrents}
            verifying={verifying.value}
            onRefresh={() => void runVerify(apiKey.value)}
          />
        )}

        <CompatibleClients />
      </main>
    </div>
  );
}
