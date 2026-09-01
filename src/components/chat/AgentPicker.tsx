/// The custom-agent picker in the chat header: a toggle showing the active
/// agent's name and a menu to switch between the default agent and any
/// user-defined agents. Owns its open state and closes on outside click.

import { createSignal, onCleanup, Show, type Accessor } from "solid-js";
import type { CustomAgent } from "../../lib/ipc";

export function AgentPicker(props: {
  agents: Accessor<CustomAgent[]>;
  activeAgentId: Accessor<string | null>;
  setActiveAgentId: (id: string | null) => void;
}) {
  const [open, setOpen] = createSignal(false);
  let el: HTMLDivElement | undefined;

  function onDocClick(e: MouseEvent) {
    if (el && !el.contains(e.target as Node) && open()) {
      setOpen(false);
    }
  }
  document.addEventListener("click", onDocClick);
  onCleanup(() => document.removeEventListener("click", onDocClick));

  return (
    <div class="agent-picker" ref={(e) => (el = e)}>
      <button
        type="button"
        class="agent-toggle"
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="true"
        aria-expanded={open()}
      >
        <span class="agent-name">
          {props.agents().find((a) => a.id === props.activeAgentId())?.name ??
            "Default"}
        </span>
        <span class="session-caret">▾</span>
      </button>
      <Show when={open()}>
        <div class="agent-menu">
          <button
            type="button"
            class="agent-item"
            classList={{ active: props.activeAgentId() === null }}
            onClick={() => {
              props.setActiveAgentId(null);
              setOpen(false);
            }}
          >
            Default
          </button>
          {props.agents().map((a) => (
            <button
              type="button"
              class="agent-item"
              classList={{ active: a.id === props.activeAgentId() }}
              onClick={() => {
                props.setActiveAgentId(a.id);
                setOpen(false);
              }}
            >
              {a.name}
            </button>
          ))}
          <Show when={props.agents().length === 0}>
            <p class="agent-menu-note">
              No custom agents. Create one in Settings.
            </p>
          </Show>
        </div>
      </Show>
    </div>
  );
}
