import type { AdminOverview, CreatedUserResponse, PublicInfo, VerifyKeyResponse } from "./types";

async function requestJson<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await fetch(url, {
    credentials: "same-origin",
    headers: {
      "Content-Type": "application/json",
      ...init?.headers,
    },
    ...init,
  });

  if (!response.ok) {
    let message = `Request failed (${response.status})`;
    try {
      const data = (await response.json()) as { error?: string };
      if (data?.error) {
        message = data.error;
      }
    } catch {
      // Ignore non-JSON error body
    }
    throw new Error(message);
  }

  return (await response.json()) as T;
}

export function fetchPublicInfo(): Promise<PublicInfo> {
  return requestJson<PublicInfo>("/api/info");
}

export function verifyApiKey(apiKey: string): Promise<VerifyKeyResponse> {
  const encoded = encodeURIComponent(apiKey.trim());
  return requestJson<VerifyKeyResponse>(`/api/verify?api_key=${encoded}`);
}

export async function checkAdminSession(): Promise<boolean> {
  try {
    const res = await fetch("/api/admin/session", { credentials: "same-origin" });
    return res.ok;
  } catch {
    return false;
  }
}

export function adminLogin(password: string): Promise<{ ok: boolean }> {
  return requestJson<{ ok: boolean }>("/api/admin/login", {
    method: "POST",
    body: JSON.stringify({ password }),
  });
}

export function adminLogout(): Promise<{ ok: boolean }> {
  return requestJson<{ ok: boolean }>("/api/admin/logout", {
    method: "POST",
  });
}

export function updateAdminPassword(password: string): Promise<{ ok: boolean }> {
  return requestJson<{ ok: boolean }>("/api/admin/password", {
    method: "POST",
    body: JSON.stringify({ password }),
  });
}

export function fetchAdminOverview(): Promise<AdminOverview> {
  return requestJson<AdminOverview>("/api/admin/overview");
}

export function createUser(username: string): Promise<CreatedUserResponse> {
  return requestJson<CreatedUserResponse>("/api/admin/users", {
    method: "POST",
    body: JSON.stringify({ username }),
  });
}

export function deleteUser(apiKey: string): Promise<{ ok: boolean }> {
  return requestJson<{ ok: boolean }>(`/api/admin/users/${encodeURIComponent(apiKey)}`, {
    method: "DELETE",
  });
}

export function deleteTorrent(hash: string): Promise<{ ok: boolean }> {
  return requestJson<{ ok: boolean }>(`/api/admin/torrents/${encodeURIComponent(hash)}`, {
    method: "DELETE",
  });
}
