import { createSignal, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

interface TreeNode {
  name: string;
  is_dir: boolean;
  children?: TreeNode[];
}

function TreeItem(props: { node: TreeNode }) {
  const [expanded, setExpanded] = createSignal(false);

  return (
    <li class="tree-item">
      <button
        type="button"
        class="tree-row"
        onClick={() => props.node.is_dir && setExpanded(!expanded())}
      >
        <span class="tree-caret">
          {props.node.is_dir ? (expanded() ? "▾" : "▸") : "·"}
        </span>
        <span class={props.node.is_dir ? "tree-dir" : "tree-file"}>
          {props.node.name}
        </span>
      </button>
      <Show when={props.node.is_dir && expanded() && props.node.children}>
        <ul class="tree-children">
          {props.node.children!.map((child) => (
            <TreeItem node={child} />
          ))}
        </ul>
      </Show>
    </li>
  );
}

function App() {
  const [tree, setTree] = createSignal<TreeNode[]>([]);
  const [treeError, setTreeError] = createSignal("");

  async function loadTree() {
    try {
      setTree(await invoke<TreeNode[]>("list_tree"));
      setTreeError("");
    } catch (err) {
      setTreeError(String(err));
    }
  }

  onMount(() => {
    loadTree();
  });

  return (
    <main class="container">
      <section class="files">
        <div class="row">
          <h2>Files in current directory</h2>
          <button type="button" onClick={loadTree}>
            Refresh
          </button>
        </div>
        {treeError() ? (
          <p class="error">{treeError()}</p>
        ) : (
          <ul class="tree">
            {tree().map((node) => (
              <TreeItem node={node} />
            ))}
          </ul>
        )}
      </section>
    </main>
  );
}

export default App;
