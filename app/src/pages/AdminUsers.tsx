import { useSignal } from "@preact/signals";
import { Loader2, UserPlus } from "lucide-preact";
import { createUser, deleteUser } from "../api";
import { AdminUserRow } from "../components/admin/AdminUserRow";
import { CreatedUserBanner } from "../components/admin/CreatedUserBanner";
import type { AdminUser, CreatedUserResponse } from "../types";
import { buildInviteUrl, copyToClipboard } from "../utils";

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

  return (
    <section class="space-y-3.5 sm:space-y-4">
      <div class="flex flex-wrap items-center justify-between gap-3 sm:gap-4">
        <div>
          <h2 class="text-lg font-semibold text-white">Users &amp; Invite Links</h2>
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
        <CreatedUserBanner
          user={lastCreatedUser.value}
          inviteUrl={buildInviteUrl(effectivePublicUrl, lastCreatedUser.value.api_key)}
          copied={copiedToken.value === "new-invite"}
          onCopyInvite={() =>
            void triggerCopy(
              "new-invite",
              buildInviteUrl(effectivePublicUrl, lastCreatedUser.value?.api_key ?? ""),
            )
          }
          onDismiss={() => {
            lastCreatedUser.value = null;
          }}
        />
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
                {users.map((user) => (
                  <AdminUserRow
                    key={user.api_key}
                    user={user}
                    effectivePublicUrl={effectivePublicUrl}
                    revealed={Boolean(revealedKeys.value[user.api_key])}
                    copiedToken={copiedToken.value}
                    confirmingDelete={confirmDeleteUserKey.value === user.api_key}
                    deleting={deletingUserKey.value === user.api_key}
                    onToggleReveal={() => toggleRevealKey(user.api_key)}
                    onCopy={(token, value) => void triggerCopy(token, value)}
                    onRequestDelete={() => {
                      confirmDeleteUserKey.value = user.api_key;
                    }}
                    onConfirmDelete={() => void handleDeleteUser(user.api_key)}
                    onCancelDelete={() => {
                      confirmDeleteUserKey.value = null;
                    }}
                  />
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </section>
  );
}
