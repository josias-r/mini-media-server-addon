import { useSignal } from "@preact/signals";
import { ArrowLeft, Eye, EyeOff, Loader2, Lock, ShieldAlert } from "lucide-preact";
import { adminLogin } from "../api";

interface AdminLoginProps {
  onSuccess: () => Promise<void>;
  onNavigate: (path: string) => void;
}

export function AdminLogin({ onSuccess, onNavigate }: AdminLoginProps) {
  const password = useSignal("");
  const showPassword = useSignal(false);
  const loading = useSignal(false);
  const error = useSignal<string | null>(null);

  async function handleLogin(e: Event) {
    e.preventDefault();
    if (!password.value) return;
    loading.value = true;
    error.value = null;
    try {
      await adminLogin(password.value);
      password.value = "";
      await onSuccess();
    } catch (err) {
      error.value = err instanceof Error ? err.message : "Invalid password";
    } finally {
      loading.value = false;
    }
  }

  return (
    <div class="relative flex min-h-screen items-center justify-center px-4">
      <div class="pointer-events-none fixed inset-0 -z-10 bg-[radial-gradient(circle_at_top,_rgba(99,102,241,0.15),transparent_50%)]" />
      <div class="w-full max-w-md rounded-2xl border border-slate-800 bg-slate-900/80 p-7 shadow-2xl shadow-black/50 backdrop-blur-md">
        <div class="mb-6 flex flex-col items-center text-center">
          <div class="mb-3 flex h-12 w-12 items-center justify-center rounded-2xl bg-gradient-to-br from-indigo-500 to-violet-600 shadow-lg shadow-indigo-500/25">
            <Lock class="h-6 w-6 text-white" />
          </div>
          <h1 class="text-xl font-bold text-white">Admin Control Center</h1>
          <p class="mt-1 text-sm text-slate-400">
            Sign in with the admin password generated in server logs.
          </p>
        </div>

        {error.value && (
          <div class="mb-4 flex items-center gap-2 rounded-xl border border-rose-500/30 bg-rose-500/10 px-3.5 py-2.5 text-sm text-rose-300">
            <ShieldAlert class="h-4 w-4 shrink-0" />
            <span>{error.value}</span>
          </div>
        )}

        <form onSubmit={handleLogin} class="space-y-4">
          <div>
            <label for="admin-password" class="mb-1.5 block text-xs font-medium text-slate-300">
              Admin Password
            </label>
            <div class="relative">
              <input
                id="admin-password"
                type={(showPassword.value ? "text" : "password") as "text"}
                value={password.value}
                onInput={(e) => {
                  password.value = (e.target as HTMLInputElement).value;
                }}
                required
                autofocus
                placeholder="Enter admin password"
                class="w-full rounded-xl border border-slate-700 bg-slate-950 py-2.5 pr-11 pl-3.5 text-sm text-white placeholder-slate-500 outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500/20"
              />
              <button
                type="button"
                aria-label={showPassword.value ? "Hide password" : "Show password"}
                onClick={() => {
                  showPassword.value = !showPassword.value;
                }}
                class="absolute inset-y-0 right-2 flex items-center px-1.5 text-slate-400 hover:text-slate-200"
              >
                {showPassword.value ? <EyeOff class="h-4 w-4" /> : <Eye class="h-4 w-4" />}
              </button>
            </div>
          </div>

          <button
            type="submit"
            disabled={loading.value}
            class="flex w-full items-center justify-center gap-2 rounded-xl bg-indigo-600 px-4 py-2.5 text-sm font-semibold text-white shadow-lg shadow-indigo-600/20 transition hover:bg-indigo-500 disabled:opacity-60"
          >
            {loading.value && <Loader2 class="h-4 w-4 animate-spin" />}
            Sign In
          </button>
        </form>

        <div class="mt-6 text-center">
          <a
            href="/"
            onClick={(e) => {
              e.preventDefault();
              onNavigate("/");
            }}
            class="inline-flex items-center gap-1.5 text-xs text-slate-400 transition hover:text-white"
          >
            <ArrowLeft class="h-3.5 w-3.5" />
            Back to User Portal
          </a>
        </div>
      </div>
    </div>
  );
}
