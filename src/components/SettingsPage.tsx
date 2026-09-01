/// A settings page. Manages the OpenRouter API key and custom agents
/// (user-defined system prompts). A back button returns to the main view.

import { createSignal, onCleanup, Show, type Accessor } from "solid-js";
import { setKey, type CustomAgent } from "../lib/ipc";

export function SettingsPage(props: {
  keyMasked: Accessor<string>;
  onKeyChange: (masked: string) => void;
  agents: Accessor<CustomAgent[]>;
  setAgents: (agents: CustomAgent[]) => void;
  // The llama.cpp base URL, owned by App. The active provider is chosen in the
  // chat panel, not here — this page only configures each provider.
  baseUrl: Accessor<string>;
  setBaseUrl: (url: string) => void;
  onBack: () => void;
}) {
  const [keyDraft, setKeyDraft] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal("");
  // Draft for the llama.cpp base URL, applied to App on change.
  const [baseUrlDraft, setBaseUrlDraft] = createSignal(props.baseUrl());
  // Transient "Saved" confirmation shown after the base URL is stored. The
  // save itself is synchronous (App persists it via its debounced effect), so
  // this is purely visual feedback — there's no async work to await.
  const [baseUrlSaved, setBaseUrlSaved] = createSignal(false);
  let baseUrlSavedTimer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => {
    if (baseUrlSavedTimer) {
      clearTimeout(baseUrlSavedTimer);
    }
  });

  /// Stores the llama.cpp base URL and flashes a brief "Saved" confirmation.
  function saveBaseUrl() {
    const url = baseUrlDraft().trim();
    if (!url) {
      return;
    }
    props.setBaseUrl(url);
    setBaseUrlSaved(true);
    if (baseUrlSavedTimer) {
      clearTimeout(baseUrlSavedTimer);
    }
    baseUrlSavedTimer = setTimeout(() => setBaseUrlSaved(false), 2000);
  }

  // Custom agent form state.
  const [agentName, setAgentName] = createSignal("");
  const [agentPrompt, setAgentPrompt] = createSignal("");
  const [editingAgentId, setEditingAgentId] = createSignal<string | null>(null);

  /// Stores the key the user typed.
  async function saveKey() {
    const key = keyDraft().trim();
    if (!key) {
      return;
    }
    setSaving(true);
    setError("");
    try {
      props.onKeyChange(await setKey(key));
      setKeyDraft("");
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }

  /// Removes the stored key.
  async function removeKey() {
    setSaving(true);
    setError("");
    try {
      props.onKeyChange(await setKey(""));
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }

  /// Creates or updates a custom agent.
  function saveAgent() {
    const name = agentName().trim();
    const prompt = agentPrompt().trim();
    if (!name || !prompt) return;
    if (editingAgentId()) {
      // Update existing.
      props.setAgents(
        props
          .agents()
          .map((a) => (a.id === editingAgentId() ? { ...a, name, prompt } : a)),
      );
    } else {
      // Create new.
      props.setAgents([
        ...props.agents(),
        { id: crypto.randomUUID(), name, prompt },
      ]);
    }
    setAgentName("");
    setAgentPrompt("");
    setEditingAgentId(null);
  }

  /// Starts editing an existing agent.
  function editAgent(agent: CustomAgent) {
    setEditingAgentId(agent.id);
    setAgentName(agent.name);
    setAgentPrompt(agent.prompt);
  }

  /// Deletes a custom agent.
  function deleteAgent(id: string) {
    props.setAgents(props.agents().filter((a) => a.id !== id));
    if (editingAgentId() === id) {
      setEditingAgentId(null);
      setAgentName("");
      setAgentPrompt("");
    }
  }

  /// Cancels the current edit.
  function cancelEdit() {
    setEditingAgentId(null);
    setAgentName("");
    setAgentPrompt("");
  }

  return (
    <div class="settings">
      <div class="row settings-header">
        <button type="button" onClick={props.onBack}>
          ← Back
        </button>
        <h2>Settings</h2>
      </div>

      <section class="settings-section">
        <h3>Model Providers</h3>
        <p class="settings-desc">
          You choose which provider to use from the chat panel. Here you
          configure each one: OpenRouter is a hosted API (needs an API key);
          llama.cpp runs a local server you point it at.
        </p>

        <h4>OpenRouter</h4>
        <p class="settings-desc">
          The API key is stored in your OS credential manager and is used to
          list the models available on your account.
        </p>
        <Show
          when={props.keyMasked()}
          fallback={
            <form
              class="key-form"
              onSubmit={(e) => {
                e.preventDefault();
                saveKey();
              }}
            >
              <input
                type="password"
                class="key-field"
                placeholder="OpenRouter API key (sk-or-…)"
                value={keyDraft()}
                onInput={(e) => setKeyDraft(e.currentTarget.value)}
              />
              <button type="submit" disabled={!keyDraft().trim() || saving()}>
                Save
              </button>
            </form>
          }
        >
          <div class="row key-managed">
            <span class="key-status" title="OpenRouter key saved">
              🔑 {props.keyMasked()}
            </span>
            <button type="button" onClick={removeKey} disabled={saving()}>
              Remove
            </button>
          </div>
        </Show>

        <h4>llama.cpp</h4>
        <p class="settings-desc">
          The base URL of your local llama.cpp server (the one that serves an
          OpenAI-compatible API). No API key is needed.
        </p>
        <form
          class="key-form"
          onSubmit={(e) => {
            e.preventDefault();
            saveBaseUrl();
          }}
        >
          <input
            type="text"
            class="key-field"
            placeholder="http://localhost:8080"
            value={baseUrlDraft()}
            onInput={(e) => setBaseUrlDraft(e.currentTarget.value)}
          />
          <button type="submit" disabled={!baseUrlDraft().trim()}>
            Save
          </button>
        </form>
        <Show when={baseUrlSaved()}>
          <p class="settings-saved">Base URL saved.</p>
        </Show>

        <Show when={error()}>
          <p class="error">{error()}</p>
        </Show>
      </section>

      <section class="settings-section">
        <h3>Custom Agents</h3>
        <p class="settings-desc">
          A custom agent is a named system prompt layered on top of the default
          agent behavior. Pick one from the chat header to use it.
        </p>

        <ul class="agent-list">
          {props.agents().map((a) => (
            <li class="agent-list-item">
              <span class="agent-list-name">{a.name}</span>
              <span class="agent-list-prompt" title={a.prompt}>
                {a.prompt}
              </span>
              <span class="agent-list-actions">
                <button type="button" onClick={() => editAgent(a)}>
                  Edit
                </button>
                <button type="button" onClick={() => deleteAgent(a.id)}>
                  Delete
                </button>
              </span>
            </li>
          ))}
          <Show when={props.agents().length === 0}>
            <li class="agent-list-empty">No custom agents yet.</li>
          </Show>
        </ul>

        <form
          class="agent-form"
          onSubmit={(e) => {
            e.preventDefault();
            saveAgent();
          }}
        >
          <h4>{editingAgentId() ? "Edit agent" : "New agent"}</h4>
          <input
            type="text"
            class="agent-field"
            placeholder="Name (e.g. Code Reviewer)"
            value={agentName()}
            onInput={(e) => setAgentName(e.currentTarget.value)}
          />
          <textarea
            class="agent-prompt-field"
            placeholder="System prompt (e.g. You are a strict code reviewer…)"
            value={agentPrompt()}
            onInput={(e) => setAgentPrompt(e.currentTarget.value)}
            rows={4}
            maxLength={4000}
          ></textarea>
          <p class="agent-prompt-count">{agentPrompt().length}/4000</p>
          <div class="row agent-form-actions">
            <button
              type="submit"
              disabled={!agentName().trim() || !agentPrompt().trim()}
            >
              {editingAgentId() ? "Save" : "Add"}
            </button>
            <Show when={editingAgentId()}>
              <button type="button" onClick={cancelEdit}>
                Cancel
              </button>
            </Show>
          </div>
        </form>
      </section>
    </div>
  );
}
