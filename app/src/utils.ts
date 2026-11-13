export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let idx = 0;
  let value = bytes;
  while (value >= 1024 && idx < units.length - 1) {
    value /= 1024;
    idx += 1;
  }
  return `${value.toFixed(idx === 0 ? 0 : 2)} ${units[idx]}`;
}

export function formatSpeed(bytesPerSec: number): string {
  if (!Number.isFinite(bytesPerSec) || bytesPerSec <= 0) return "0 B/s";
  return `${formatBytes(bytesPerSec)}/s`;
}

export function parseUtcDate(value: string | number | null | undefined): Date | null {
  if (value === null || value === undefined || value === "") return null;
  if (typeof value === "number") {
    if (value <= 0) return null;
    return new Date(value * 1000);
  }
  const normalized =
    value.includes("T") || value.endsWith("Z") ? value : `${value.replace(" ", "T")}Z`;
  const date = new Date(normalized);
  return Number.isNaN(date.getTime()) ? null : date;
}

export function formatRelativeTime(value: string | number | null | undefined): string {
  const date = parseUtcDate(value);
  if (!date) return "Never";

  const diffMs = date.getTime() - Date.now();
  const absSecs = Math.round(Math.abs(diffMs) / 1000);
  if (absSecs < 45) return diffMs <= 0 ? "just now" : "in a moment";

  const rtf = new Intl.RelativeTimeFormat("en", { numeric: "auto" });
  const diffMins = Math.round(diffMs / 60_000);
  if (Math.abs(diffMins) < 60) return rtf.format(diffMins, "minute");

  const diffHours = Math.round(diffMs / 3_600_000);
  if (Math.abs(diffHours) < 24) return rtf.format(diffHours, "hour");

  const diffDays = Math.round(diffMs / 86_400_000);
  return rtf.format(diffDays, "day");
}

export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0 || seconds >= 8_640_000) return "—";
  const hrs = Math.floor(seconds / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  const secs = Math.floor(seconds % 60);
  if (hrs > 0) return `${hrs}h ${mins}m`;
  if (mins > 0) return `${mins}m ${secs}s`;
  return `${secs}s`;
}

export function formatStateLabel(state: string): {
  label: string;
  tone: "emerald" | "indigo" | "amber" | "rose" | "slate";
} {
  const normalized = state.toLowerCase();
  if (
    normalized.includes("upload") ||
    normalized.includes("seed") ||
    normalized === "pausedup" ||
    normalized === "stoppedup"
  ) {
    return { label: "Ready / Seeding", tone: "emerald" };
  }
  if (
    normalized.includes("dl") ||
    normalized === "downloading" ||
    normalized === "metadl" ||
    normalized === "forceddl"
  ) {
    return { label: "Downloading", tone: "indigo" };
  }
  if (normalized.includes("stalled") || normalized.includes("queued")) {
    return {
      label: normalized.includes("up") ? "Ready / Queued" : "Queued / Stalled",
      tone: "amber",
    };
  }
  if (normalized.includes("error") || normalized === "missingfiles") {
    return { label: "Error", tone: "rose" };
  }
  return { label: state || "Unknown", tone: "slate" };
}

export function isDownloadingState(state: string, progress: number): boolean {
  if (progress >= 0.999) return false;
  const normalized = state.toLowerCase();
  return (
    normalized.includes("dl") ||
    normalized === "downloading" ||
    normalized === "stalleddl" ||
    normalized === "queueddl" ||
    normalized === "checkingdl"
  );
}

export function buildManifestUrl(publicUrl: string, apiKey: string): string {
  const base = publicUrl.replace(/\/+$/, "");
  const key = apiKey.trim() || "YOUR_API_KEY";
  return `${base}/${key}/manifest.json`;
}

export function buildStremioDeepLink(publicUrl: string, apiKey: string): string {
  const manifestUrl = buildManifestUrl(publicUrl, apiKey);
  return `stremio://${manifestUrl.replace(/^https?:\/\//, "")}`;
}

export function buildInviteUrl(publicUrl: string, apiKey: string): string {
  const base = publicUrl.replace(/\/+$/, "");
  return `${base}/?key=${encodeURIComponent(apiKey)}`;
}

export async function copyToClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
