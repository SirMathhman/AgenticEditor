/// The "Tools" popover in the chat header: a toggle showing how many tools the
/// agent can call, and a menu listing them. The tool registry is static for a
/// given build, so the list is loaded once on mount. Owns its open state and
/// closes on outside click.

import { createSignal, Index, onCleanup, onMount, Show } from "solid-js";
import { listTools, type ToolInfo } from "../../lib/ipc";

export function ToolsPopover() {
  const [tools, setTools] = createSignal<ToolInfo[]>([]);
  const [open, setOpen] = createSignal(false);
  let el: HTMLDivElement | undefined;

  onMount(() => {
    void listTools().then(setTools);
  });

  function onDocClick(e: MouseEvent) {
    if (el && !el.contains(e.target as Node) && open()) {
      setOpen(false);
    }
  }
  document.addEventListener("click", onDocClick);
  onCleanup(() => document.removeEventListener("click", onDocClick));

  return (
    <div class="tools-picker" ref={(e) => (el = e)}>
      <button
        type="button"
        class="tools-toggle"
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="true"
        aria-expanded={open()}
      >
        <span>Tools</span>
        <span class="tools-count">{tools().length}</span>
      </button>
      <Show when={open()}>
        <div class="tools-menu">
          <p class="tools-menu-note">
            The agent can call these tools while it works.
          </p>
          <ul>
            <Index each={tools()}>
              {(t) => (
                <li class="tools-item">
                  <span class="tools-item-name">{t().name}</span>
                  <span class="tools-item-desc">{t().description}</span>
                </li>
              )}
            </Index>
          </ul>
        </div>
      </Show>
    </div>
  );
}
