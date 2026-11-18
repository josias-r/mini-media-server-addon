import { Eye, EyeOff, KeyRound, Loader2, X } from "lucide-preact";
import { Badge } from "./Badge";

export interface ApiKeyInputProps {
  apiKey: string;
  showKey: boolean;
  onApiKeyChange: (nextKey: string) => void;
  onToggleShowKey: () => void;
  onClearKey: () => void;
}

/** Masked API key text input with clear and visibility toggle buttons. */
export function ApiKeyInput({
  apiKey,
  showKey,
  onApiKeyChange,
  onToggleShowKey,
  onClearKey,
}: ApiKeyInputProps) {
  return (
    <div class="relative">
      <input
        id="api-key-input"
        type={(showKey ? "text" : "password") as "text"}
        value={apiKey}
        onInput={(e) => {
          onApiKeyChange((e.target as HTMLInputElement).value);
        }}
        placeholder="Paste your API key (e.g. 550e8400-e29b-41d4-a716-446655440000)"
        autocomplete="off"
        spellcheck={false}
        class="w-full rounded-xl border border-slate-700/80 bg-slate-950/90 py-3 pr-20 pl-3.5 font-mono text-base text-slate-100 placeholder-slate-500 transition outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500/20 sm:pl-4 sm:text-sm"
      />
      <div class="absolute inset-y-0 right-1.5 flex items-center gap-0.5">
        {apiKey && (
          <button
            type="button"
            onClick={onClearKey}
            title="Clear saved key"
            class="flex h-9 w-9 items-center justify-center rounded-lg text-slate-400 transition hover:bg-slate-800 hover:text-slate-200"
          >
            <X class="h-4 w-4" />
          </button>
        )}
        <button
          type="button"
          onClick={onToggleShowKey}
          title={showKey ? "Hide API key" : "Show API key"}
          class="flex h-9 w-9 items-center justify-center rounded-lg text-slate-400 transition hover:bg-slate-800 hover:text-slate-200"
        >
          {showKey ? <EyeOff class="h-4 w-4" /> : <Eye class="h-4 w-4" />}
        </button>
      </div>
    </div>
  );
}

interface ApiKeyEditorDrawerProps extends ApiKeyInputProps {
  open: boolean;
  borderClass: string;
  verifying: boolean;
}

/** Collapsible drawer for editing or clearing a saved API key inside onboarding cards. */
export function ApiKeyEditorDrawer({
  open,
  borderClass,
  verifying,
  ...inputProps
}: ApiKeyEditorDrawerProps) {
  if (!open) return null;
  return (
    <div class={`mt-4 border-t pt-4 ${borderClass}`}>
      <div class="flex flex-wrap items-center justify-between gap-2">
        <label
          for="api-key-input"
          class="flex items-center gap-2 text-xs font-medium text-slate-300"
        >
          <KeyRound class="h-3.5 w-3.5 text-indigo-400" />
          Your Saved API Key
        </label>
        {verifying && (
          <Badge tone="slate">
            <Loader2 class="h-3 w-3 animate-spin" />
            Checking...
          </Badge>
        )}
      </div>
      <div class="mt-2">
        <ApiKeyInput {...inputProps} />
      </div>
    </div>
  );
}
