import { useSignal } from "@preact/signals";
import {
  Check,
  ClipboardCopy,
  Eye,
  EyeOff,
  KeyRound,
  Link2,
  Loader2,
  Trash2,
  UserPlus,
  X,
} from "lucide-preact";
import { createUser, deleteUser } from "../api";
import type { AdminUser, CreatedUserResponse } from "../types";
import { buildInviteUrl, buildManifestUrl, copyToClipboard, formatRelativeTime } from "../utils";

interface AdminUsersProps {
  users: AdminUser[];
  effectivePublicUrl: string;
  onUserMutated: () => Promise<void>;
  onError: (msg: string) => void;
}

/** Dedicated admin page for creating users, sharing invite links, and revoking API keys. */
export function AdminUsers({ users, effectivePublicUrl, onUserMutated, onError }: AdminUsersProps) {
  const newUsername = useSignal("");
  const creatingUser = useSignal(false);
  const createUserError = useSignal<string | null>(null);
  const lastCreatedUser = useSignal<CreatedUserResponse | null>(null);

  const revealedKeys = useSignal<Record<string, boolean>>({});
  const copiedToken = useSignal<string | null>(null);
  const confirmDeleteUserKey = useSignal<string | null>(null);
  const deletingUserKey = useSignal<string | null>(null);

  async function handleCreateUser(e: Event) {
    e.preventDefault();
    const trimmed = newUsername.value.trim();
    if (!trimmed) return;
    creatingUser.value = true;
    createUserError.value = null;
    try {
      const created = await createUser(trimmed);
      lastCreatedUser.value = created;
      newUsername.value = "";
      await onUserMutated();
    } catch (err) {
      createUserError.value = err instanceof Error ? err.message : "Failed to create user";
    } finally {
      creatingUser.value = false;
    }
  }

  async function handleDeleteUser(apiKey: string) {
    deletingUserKey.value = apiKey;
    try {
      await deleteUser(apiKey);
      confirmDeleteUserKey.value = null;
      if (lastCreatedUser.value?.api_key === apiKey) {
        lastCreatedUser.value = null;
      }
      await onUserMutated();
    } catch (err) {
      onError(err instanceof Error ? err.message : "Failed to delete user");
    } finally {
      deletingUserKey.value = null;
    }
  }

  async function triggerCopy(token: string, value: string) {
    const ok = await copyToClipboard(value);
    if (ok) {
      copiedToken.value = token;
      window.setTimeout(() => {
        if (copiedToken.value === token) {
          copiedToken.value = null;
        }
      }, 2000);
    }
  }

  function toggleRevealKey(apiKey: string) {
    revealedKeys.value = {
      ...revealedKeys.value,
      [apiKey]: !revealedKeys.value[apiKey],
    };
  }

  function maskKey(key: string): string {
    if (key.length <= 12) return "••••••••••••";
    return `${key.slice(0, 8)}••••••••••••${key.slice(-6)}`;
  }

  return (
    <section class="space-y-3.5 sm:space-y-4">
      <div class="flex flex-wrap items-center justify-between gap-3 sm:gap-4">
        <div>
          <h2 class="text-lg font-semibold text-white">Users & Invite Links</h2>
          <p class="text-xs text-slate-400">
            Create API keys and share 1-click invite links that auto-configure the user portal.
          </p>
        </div>

        <form onSubmit={handleCreateUser} class="flex w-full gap-2 sm:w-auto">
          <input
            type="text"
            value={newUsername.value}
            onInput={(e) => {
              newUsername.value = (e.target as HTMLInputElement).value;
            }}
            placeholder="New username..."
            required
            class="min-w-0 flex-1 rounded-xl border border-slate-700 bg-slate-900 px-3.5 py-2.5 text-base text-white placeholder-slate-500 outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500/20 sm:w-56 sm:py-2 sm:text-sm"
          />
          <button
            type="submit"
            disabled={creatingUser.value}
            class="inline-flex shrink-0 items-center gap-1.5 rounded-xl bg-indigo-600 px-4 py-2.5 text-sm font-semibold whitespace-nowrap text-white shadow-lg shadow-indigo-600/20 transition hover:bg-indigo-500 disabled:opacity-60 sm:py-2"
          >
            {creatingUser.value ? (
              <Loader2 class="h-4 w-4 animate-spin" />
            ) : (
              <UserPlus class="h-4 w-4" />
            )}
            Add User
          </button>
        </form>
      </div>

      {createUserError.value && (
        <div class="rounded-xl border border-rose-500/30 bg-rose-500/10 px-4 py-2.5 text-sm text-rose-300">
          {createUserError.value}
        </div>
      )}

      {lastCreatedUser.value && (
        <div class="rounded-2xl border border-emerald-500/30 bg-emerald-500/10 p-4">
          <div class="flex flex-wrap items-center justify-between gap-3">
            <div>
              <p class="text-sm font-semibold text-emerald-300">
                User “{lastCreatedUser.value.username}” created!
              </p>
              <p class="mt-0.5 text-xs text-emerald-200/80">
                Share the invite link below — it fills in their API key automatically.
              </p>
            </div>
            <div class="flex w-full items-center gap-2 sm:w-auto">
              <button
                type="button"
                onClick={() =>
                  void triggerCopy(
                    "new-invite",
                    buildInviteUrl(effectivePublicUrl, lastCreatedUser.value?.api_key ?? ""),
                  )
                }
                class="inline-flex flex-1 items-center justify-center gap-1.5 rounded-lg bg-emerald-600 px-3 py-2 text-xs font-semibold text-white transition hover:bg-emerald-500 sm:flex-initial sm:py-1.5"
              >
                {copiedToken.value === "new-invite" ? (
                  <>
                    <Check class="h-3.5 w-3.5" />
                    Copied Invite!
                  </>
                ) : (
                  <>
                    <Link2 class="h-3.5 w-3.5" />
                    Copy Invite Link
                  </>
                )}
              </button>
              <button
                type="button"
                onClick={() => {
                  lastCreatedUser.value = null;
                }}
                class="rounded-lg p-1.5 text-emerald-300/70 hover:bg-emerald-500/20 hover:text-emerald-200 sm:p-1"
              >
                <X class="h-4 w-4" />
              </button>
            </div>
          </div>
          <div class="mt-3 rounded-lg bg-slate-950/60 px-3 py-2 font-mono text-xs break-all text-emerald-200">
            {buildInviteUrl(effectivePublicUrl, lastCreatedUser.value.api_key)}
          </div>
        </div>
      )}

      <div class="overflow-hidden rounded-2xl border border-slate-800 bg-slate-900/70">
        {users.length === 0 ? (
          <div class="p-8 text-center text-sm text-slate-400">
            No users yet. Create one above to generate an API key.
          </div>
        ) : (
          <div class="sm:overflow-x-auto">
            <table class="block w-full text-left text-sm sm:table">
              <thead class="hidden border-b border-slate-800 bg-slate-950/50 text-xs uppercase tracking-wider text-slate-400 sm:table-header-group">
                <tr>
                  <th class="px-4 py-3 font-medium">User</th>
                  <th class="px-4 py-3 font-medium">API Key</th>
                  <th class="px-4 py-3 font-medium">Activity</th>
                  <th class="px-4 py-3 font-medium">Created</th>
                  <th class="px-4 py-3 text-right font-medium">Actions</th>
                </tr>
              </thead>
              <tbody class="block divide-y divide-slate-800/80 sm:table-row-group">
                {users.map((user) => {
                  const revealed = Boolean(revealedKeys.value[user.api_key]);
                  const inviteToken = `invite-${user.api_key}`;
                  const manifestToken = `manifest-${user.api_key}`;
                  const keyToken = `key-${user.api_key}`;
                  const confirming = confirmDeleteUserKey.value === user.api_key;
                  const deleting = deletingUserKey.value === user.api_key;

                  return (
                    <tr
                      key={user.api_key}
                      class="flex flex-col gap-2.5 p-4 transition hover:bg-slate-800/30 sm:table-row sm:p-0"
                    >
                      <td class="flex items-center justify-between gap-2 font-medium text-white sm:table-cell sm:px-4 sm:py-3.5">
                        <span class="truncate">{user.username}</span>
                        <span class="shrink-0 text-xs font-normal text-slate-400 sm:hidden">
                          Created {formatRelativeTime(user.created_at)}
                        </span>
                      </td>
                      <td class="block sm:table-cell sm:px-4 sm:py-3.5">
                        <div class="flex items-center justify-between gap-1.5 rounded-xl border border-slate-800/80 bg-slate-950/80 px-3 py-2 sm:justify-start sm:rounded-none sm:border-0 sm:bg-transparent sm:p-0">
                          <code class="min-w-0 truncate font-mono text-xs text-indigo-300 sm:rounded-md sm:bg-slate-950/80 sm:px-2 sm:py-1">
                            {revealed ? user.api_key : maskKey(user.api_key)}
                          </code>
                          <button
                            type="button"
                            onClick={() => toggleRevealKey(user.api_key)}
                            title={revealed ? "Hide key" : "Reveal key"}
                            class="shrink-0 rounded-md p-1.5 text-slate-400 transition hover:bg-slate-800 hover:text-slate-200 sm:p-1"
                          >
                            {revealed ? (
                              <EyeOff class="h-3.5 w-3.5" />
                            ) : (
                              <Eye class="h-3.5 w-3.5" />
                            )}
                          </button>
                        </div>
                      </td>
                      <td class="flex items-center justify-between text-xs text-slate-300 tabular-nums sm:table-cell sm:px-4 sm:py-3.5">
                        <div>
                          {user.watch_count} stream{user.watch_count === 1 ? "" : "s"}
                        </div>
                        <div class="text-slate-500">
                          Last: {formatRelativeTime(user.last_watched_at)}
                        </div>
                      </td>
                      <td class="hidden text-xs whitespace-nowrap text-slate-400 sm:table-cell sm:px-4 sm:py-3.5">
                        {formatRelativeTime(user.created_at)}
                      </td>
                      <td class="block pt-1 sm:table-cell sm:px-4 sm:py-3.5 sm:pt-3.5">
                        <div class="flex flex-wrap items-center justify-end gap-1.5">
                          <button
                            type="button"
                            onClick={() =>
                              void triggerCopy(
                                inviteToken,
                                buildInviteUrl(effectivePublicUrl, user.api_key),
                              )
                            }
                            title="Copy 1-click user invite link"
                            class="inline-flex flex-1 items-center justify-center gap-1 rounded-lg border border-slate-700 bg-slate-800/80 px-2.5 py-2 text-xs font-medium text-slate-200 transition hover:border-indigo-500/50 hover:text-white sm:flex-initial sm:py-1"
                          >
                            {copiedToken.value === inviteToken ? (
                              <>
                                <Check class="h-3 w-3 text-emerald-400" />
                                Invite
                              </>
                            ) : (
                              <>
                                <Link2 class="h-3 w-3 text-indigo-400" />
                                Invite
                              </>
                            )}
                          </button>

                          <button
                            type="button"
                            onClick={() =>
                              void triggerCopy(
                                manifestToken,
                                buildManifestUrl(effectivePublicUrl, user.api_key),
                              )
                            }
                            title="Copy Stremio manifest URL"
                            class="inline-flex flex-1 items-center justify-center gap-1 rounded-lg border border-slate-700 bg-slate-800/80 px-2.5 py-2 text-xs font-medium text-slate-200 transition hover:border-slate-600 hover:text-white sm:flex-initial sm:py-1"
                          >
                            {copiedToken.value === manifestToken ? (
                              <>
                                <Check class="h-3 w-3 text-emerald-400" />
                                Manifest
                              </>
                            ) : (
                              <>
                                <ClipboardCopy class="h-3 w-3 text-slate-400" />
                                Manifest
                              </>
                            )}
                          </button>

                          <button
                            type="button"
                            onClick={() => void triggerCopy(keyToken, user.api_key)}
                            title="Copy raw API key"
                            class="inline-flex items-center justify-center gap-1 rounded-lg border border-slate-700 bg-slate-800/80 p-2 text-xs font-medium text-slate-300 transition hover:border-slate-600 hover:text-white sm:px-2 sm:py-1"
                          >
                            {copiedToken.value === keyToken ? (
                              <Check class="h-3.5 w-3.5 text-emerald-400 sm:h-3 sm:w-3" />
                            ) : (
                              <KeyRound class="h-3.5 w-3.5 text-slate-400 sm:h-3 sm:w-3" />
                            )}
                          </button>

                          {confirming ? (
                            <div class="inline-flex items-center gap-1">
                              <button
                                type="button"
                                disabled={deleting}
                                onClick={() => void handleDeleteUser(user.api_key)}
                                class="rounded-lg bg-rose-600 px-2.5 py-2 text-xs font-semibold text-white transition hover:bg-rose-500 disabled:opacity-60 sm:py-1"
                              >
                                {deleting ? "Deleting..." : "Confirm"}
                              </button>
                              <button
                                type="button"
                                onClick={() => {
                                  confirmDeleteUserKey.value = null;
                                }}
                                class="rounded-lg border border-slate-700 px-2.5 py-2 text-xs text-slate-300 hover:bg-slate-800 sm:px-2 sm:py-1"
                              >
                                Cancel
                              </button>
                            </div>
                          ) : (
                            <button
                              type="button"
                              onClick={() => {
                                confirmDeleteUserKey.value = user.api_key;
                              }}
                              title="Revoke user"
                              class="rounded-lg border border-rose-500/30 bg-rose-500/10 p-2 text-rose-300 transition hover:bg-rose-500/20 sm:p-1.5"
                            >
                              <Trash2 class="h-3.5 w-3.5" />
                            </button>
                          )}
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </section>
  );
}
