import { useSignal } from "@preact/signals";
import { KeyRound, Loader2, X } from "lucide-preact";
import { updateAdminPassword } from "../api";

interface PasswordModalProps {
  onClose: () => void;
}

export function PasswordModal({ onClose }: PasswordModalProps) {
  const newPassword = useSignal("");
  const confirmPassword = useSignal("");
  const saving = useSignal(false);
  const feedback = useSignal<{ type: "ok" | "err"; text: string } | null>(null);

  async function handleSave(e: Event) {
    e.preventDefault();
    feedback.value = null;
    if (newPassword.value.length < 4) {
      feedback.value = { type: "err", text: "Password must be at least 4 characters." };
      return;
    }
    if (newPassword.value !== confirmPassword.value) {
      feedback.value = { type: "err", text: "Passwords do not match." };
      return;
    }

    saving.value = true;
    try {
      await updateAdminPassword(newPassword.value);
      newPassword.value = "";
      confirmPassword.value = "";
      feedback.value = { type: "ok", text: "Admin password updated successfully." };
    } catch (err) {
      feedback.value = {
        type: "err",
        text: err instanceof Error ? err.message : "Failed to update password",
      };
    } finally {
      saving.value = false;
    }
  }

  return (
    <div class="fixed inset-0 z-50 flex items-end justify-center bg-black/70 p-3 backdrop-blur-sm sm:items-center sm:p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Change Admin Password"
        class="w-full max-w-md rounded-2xl border border-slate-800 bg-slate-900 p-5 shadow-2xl sm:p-6"
      >
        <div class="mb-4 flex items-center justify-between">
          <div class="flex items-center gap-2">
            <KeyRound class="h-5 w-5 text-indigo-400" />
            <h3 class="text-base font-semibold text-white">Change Admin Password</h3>
          </div>
          <button
            type="button"
            aria-label="Close password modal"
            onClick={onClose}
            class="flex h-8 w-8 items-center justify-center rounded-lg text-slate-400 hover:bg-slate-800 hover:text-white"
          >
            <X class="h-4 w-4" />
          </button>
        </div>

        {feedback.value && (
          <div
            class={`mb-4 rounded-xl border px-3.5 py-2.5 text-xs font-medium ${
              feedback.value.type === "ok"
                ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-300"
                : "border-rose-500/30 bg-rose-500/10 text-rose-300"
            }`}
          >
            {feedback.value.text}
          </div>
        )}

        <form onSubmit={handleSave} class="space-y-4">
          <div>
            <label for="new-admin-password" class="mb-1 block text-xs font-medium text-slate-300">
              New Password
            </label>
            <input
              id="new-admin-password"
              type="password"
              value={newPassword.value}
              onInput={(e) => {
                newPassword.value = (e.target as HTMLInputElement).value;
              }}
              required
              minLength={4}
              placeholder="At least 4 characters"
              class="w-full rounded-xl border border-slate-700 bg-slate-950 px-3.5 py-2.5 text-base text-white placeholder-slate-500 outline-none focus:border-indigo-500 sm:text-sm"
            />
          </div>

          <div>
            <label
              for="confirm-admin-password"
              class="mb-1 block text-xs font-medium text-slate-300"
            >
              Confirm New Password
            </label>
            <input
              id="confirm-admin-password"
              type="password"
              value={confirmPassword.value}
              onInput={(e) => {
                confirmPassword.value = (e.target as HTMLInputElement).value;
              }}
              required
              minLength={4}
              placeholder="Repeat new password"
              class="w-full rounded-xl border border-slate-700 bg-slate-950 px-3.5 py-2.5 text-base text-white placeholder-slate-500 outline-none focus:border-indigo-500 sm:text-sm"
            />
          </div>

          <div class="flex justify-end gap-2 pt-2">
            <button
              type="button"
              onClick={onClose}
              class="flex-1 justify-center rounded-xl border border-slate-700 px-4 py-2.5 text-xs font-medium text-slate-300 hover:bg-slate-800 sm:flex-initial sm:py-2"
            >
              Close
            </button>
            <button
              type="submit"
              disabled={saving.value}
              class="inline-flex flex-1 items-center justify-center gap-1.5 rounded-xl bg-indigo-600 px-4 py-2.5 text-xs font-semibold text-white hover:bg-indigo-500 disabled:opacity-60 sm:flex-initial sm:py-2"
            >
              {saving.value && <Loader2 class="h-3.5 w-3.5 animate-spin" />}
              Save Password
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
