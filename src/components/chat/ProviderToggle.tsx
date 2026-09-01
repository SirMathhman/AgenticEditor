/// The provider switch in the chat header: a compact segmented control to
/// choose where the agent's models come from (OpenRouter or llama.cpp). This is
/// where the user picks the provider to *use*; the Settings page only explains
/// how to configure each one. Owns no state — it reads and writes the lifted
/// `provider` signal passed in from App.

import type { Accessor, Setter } from "solid-js";
import type { Provider } from "../../lib/ipc";

export function ProviderToggle(props: {
  provider: Accessor<Provider>;
  setProvider: Setter<Provider>;
}) {
  return (
    <div class="provider-toggle provider-toggle-compact" role="group">
      <button
        type="button"
        classList={{ active: props.provider() === "open-router" }}
        onClick={() => props.setProvider("open-router")}
      >
        OpenRouter
      </button>
      <button
        type="button"
        classList={{ active: props.provider() === "llama-cpp" }}
        onClick={() => props.setProvider("llama-cpp")}
      >
        llama.cpp
      </button>
    </div>
  );
}
