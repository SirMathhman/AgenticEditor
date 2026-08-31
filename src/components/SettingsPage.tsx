/// A simple settings page. For now it manages the OpenRouter API key: the
/// user can enter a key (stored in the OS credential manager via the backend),
/// see the masked key, or remove it. A back button returns to the main view.

import { createSignal, Show, type Accessor } from "solid-js";
import { setKey } from "../lib/ipc";

export function SettingsPage(props: {
  keyMasked: Accessor<string>;
  onKeyChange: (masked: string) => void;
  onBack: () => void;
}) {
  const [keyDraft, setKeyDraft] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal("");

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

  return (
    <div class="settings">
      <div class="row settings-header">
        <button type="button" onClick={props.onBack}>
          ← Back
        </button>
        <h2>Settings</h2>
      </div>

      <section class="settings-section">
        <h3>OpenRouter</h3>
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
        <Show when={error()}>
          <p class="error">{error()}</p>
        </Show>
      </section>
    </div>
  );
}
