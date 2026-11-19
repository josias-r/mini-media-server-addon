import { ExternalLink, Tv } from "lucide-preact";

const CLIENTS = [
  {
    name: "Stremio",
    platforms: "macOS • Linux • Windows • Android • Android TV",
    description:
      "The official Stremio client with native addon support and seamless playback across desktop, mobile, and TV.",
    url: "https://www.stremio.com/downloads",
    cta: "Download Stremio",
  },
  {
    name: "Nuvio",
    platforms: "Android • Android TV • macOS • Linux • Windows • Samsung TV • LG TV",
    description:
      "A modern cross-platform Stremio-compatible media center for mobile, desktop, and smart TVs.",
    url: "https://nuvio.tv/",
    cta: "Visit Nuvio",
  },
  {
    name: "Harbor (beta)",
    platforms: "macOS • Linux • Windows",
    description:
      "An open-source, customizable desktop client built for the Stremio addon ecosystem.",
    url: "https://github.com/Harborofficial/Harbor",
    cta: "View on GitHub",
  },
];

/** Cards showcasing Stremio-compatible client applications across platforms. */
export function CompatibleClients() {
  return (
    <section id="compatible-clients" class="scroll-mt-20 space-y-3.5 sm:space-y-4">
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
  );
}
