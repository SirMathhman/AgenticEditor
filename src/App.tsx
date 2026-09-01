/// The application shell: a toolbar (open/close folder, settings), and either
/// the main view (file browser + chat panel) or the settings page. It owns the
/// shared state that spans those views — the open root, recent roots, the
/// masked API key, custom agents, and the persisted model/provider settings —
/// and writes the whole settings shape to disk whenever any part changes.

import { createEffect, createSignal, onMount, Show } from "solid-js";
import { ChatPanel, FALLBACK_MODEL_ID } from "./components/ChatPanel";
import { FileBrowser } from "./components/FileBrowser";
import { SettingsPage } from "./components/SettingsPage";
import { startPaneResize } from "./lib/resize";
import {
  closeRoot,
  getKey,
  getSettings,
  pickRootFolder,
  recentRoots,
  saveSettings,
  setRoot,
  type CustomAgent,
  type Provider,
  type RecentRoot,
} from "./lib/ipc";
import "./App.css";

/// The default base URL for a local llama.cpp server, used when the user has
/// not configured one.
const DEFAULT_LLAMA_BASE_URL = "http://localhost:8080";

function App() {
  const [rootPath, setRootPath] = createSignal("");
  const [recent, setRecent] = createSignal<RecentRoot[]>([]);
  const [chatWidth, setChatWidth] = createSignal(24);
  const [view, setView] = createSignal<"main" | "settings">("main");
  // The masked OpenRouter key, owned here so the chat panel and settings page
  // share one source of truth and stay in sync when the key changes.
  const [keyMasked, setKeyMasked] = createSignal("");
  // Custom agents (user-defined system prompts), owned here so the chat panel
  // and settings page share one source of truth.
  const [agents, setAgents] = createSignal<CustomAgent[]>([]);
  const [activeAgentId, setActiveAgentId] = createSignal<string | null>(null);
  // The selected chat model. Owned here (not in ChatPanel) so App is the
  // single owner of the whole persisted settings shape and writes it whole.
  const [modelId, setModelId] = createSignal(FALLBACK_MODEL_ID);
  // The model provider and (for llama.cpp) its base URL. Owned here alongside
  // the model id so the whole settings shape is written atomically.
  const [provider, setProvider] = createSignal<Provider>("open-router");
  const [baseUrl, setBaseUrl] = createSignal(DEFAULT_LLAMA_BASE_URL);
  // True once the user has explicitly chosen a model (or a persisted model id
  // has been restored). Until then the model id is a fallback default and must
  // not be written to disk.
  const [modelChosen, setModelChosen] = createSignal(false);
  // Set once the persisted settings have been loaded, so the save effect below
  // doesn't overwrite them with defaults before the load resolves.
  const [settingsLoaded, setSettingsLoaded] = createSignal(false);

  async function loadRecent() {
    try {
      setRecent(await recentRoots());
    } catch {
      setRecent([]);
    }
  }

  async function openRecent(path: string) {
    try {
      const newRoot = await setRoot(path);
      setRootPath(newRoot);
    } catch {
      // A failed root switch leaves the previous root in place; the file
      // browser keeps showing it.
    }
  }

  async function openFolder() {
    try {
      const newRoot = await pickRootFolder(rootPath() || undefined);
      if (newRoot === null) {
        return; // user cancelled
      }
      setRootPath(newRoot);
    } catch {
      // A failed folder pick leaves the previous root in place.
    }
  }

  async function closeFolder() {
    try {
      await closeRoot();
      setRootPath("");
      loadRecent();
    } catch {
      // If closing fails, keep the current root open.
    }
  }

  onMount(async () => {
    loadRecent();
    try {
      setKeyMasked((await getKey()) ?? "");
    } catch {
      // No key stored; the empty value is already in place.
    }
    try {
      const s = await getSettings();
      setAgents(s.agents ?? []);
      setActiveAgentId(s.active_agent_id ?? null);
      if (s.provider) {
        setProvider(s.provider);
      }
      if (s.base_url) {
        setBaseUrl(s.base_url);
      }
      if (s.model_id) {
        setModelId(s.model_id);
        setModelChosen(true);
      }
    } catch {
      // No settings stored yet; defaults are already in place.
    }
    // Persist the whole settings shape (model, agents, active agent) whenever
    // any part changes. This lives in App — the single owner of the settings
    // state — so changes made on the Settings page are saved even while the
    // chat panel is unmounted (the Settings view replaces it).
    setSettingsLoaded(true);
  });

  createEffect(() => {
    if (settingsLoaded()) {
      void saveSettings({
        model_id: modelChosen() ? modelId() : null,
        agents: agents(),
        active_agent_id: activeAgentId(),
        provider: provider(),
        base_url: baseUrl(),
      });
    }
  });

  return (
    <main class="container">
      <nav class="toolbar">
        <button type="button" class="btn-primary" onClick={openFolder}>
          File
        </button>
        <Show when={rootPath()}>
          <button type="button" onClick={closeFolder}>
            Close
          </button>
        </Show>
        <span class="toolbar-root" title={rootPath()}>
          {rootPath() || "No folder open"}
        </span>
        <button
          type="button"
          class="toolbar-settings"
          onClick={() => setView("settings")}
        >
          Settings
        </button>
      </nav>
      <Show when={view() === "main"}>
        <div class="main-row">
          <FileBrowser
            rootPath={rootPath}
            recent={recent}
            onOpenRecent={openRecent}
            onRootChange={setRootPath}
          />

          <div
            class="divider"
            role="separator"
            aria-orientation="vertical"
            onPointerDown={(e) =>
              startPaneResize(
                e,
                { get: chatWidth, set: setChatWidth },
                {
                  minEm: 16,
                  maxEm: 60,
                  invert: true,
                },
              )
            }
          ></div>

          <section
            class="chat-panel"
            style={{ "flex-basis": `${chatWidth()}em` }}
          >
            <ChatPanel
              keyMasked={keyMasked}
              rootPath={rootPath}
              agents={agents}
              setAgents={setAgents}
              activeAgentId={activeAgentId}
              setActiveAgentId={setActiveAgentId}
              modelId={modelId}
              setModelId={setModelId}
              modelChosen={modelChosen}
              setModelChosen={setModelChosen}
              provider={provider}
              setProvider={setProvider}
              baseUrl={baseUrl}
            />
          </section>
        </div>
      </Show>
      <Show when={view() === "settings"}>
        <SettingsPage
          keyMasked={keyMasked}
          onKeyChange={setKeyMasked}
          agents={agents}
          setAgents={setAgents}
          baseUrl={baseUrl}
          setBaseUrl={setBaseUrl}
          onBack={() => setView("main")}
        />
      </Show>
    </main>
  );
}

export default App;
