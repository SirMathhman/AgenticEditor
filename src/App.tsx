import { createSignal, onMount, Show } from "solid-js";
import logo from "./assets/logo.svg";
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
  const [greetMsg, setGreetMsg] = createSignal("");
  const [name, setName] = createSignal("");
  const [tree, setTree] = createSignal<TreeNode[]>([]);
  const [treeError, setTreeError] = createSignal("");

  async function greet() {
    // Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
    setGreetMsg(await invoke("greet", { name: name() }));
  }

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
      <h1>Welcome to Tauri + Solid</h1>

      <div class="row">
        <a href="https://vite.dev" target="_blank">
          <img src="/vite.svg" class="logo vite" alt="Vite logo" />
        </a>
        <a href="https://tauri.app" target="_blank">
          <img src="/tauri.svg" class="logo tauri" alt="Tauri logo" />
        </a>
        <a href="https://solidjs.com" target="_blank">
          <img src={logo} class="logo solid" alt="Solid logo" />
        </a>
      </div>
      <p>Click on the Tauri, Vite, and Solid logos to learn more.</p>

      <form
        class="row"
        onSubmit={(e) => {
          e.preventDefault();
          greet();
        }}
      >
        <input
          id="greet-input"
          onChange={(e) => setName(e.currentTarget.value)}
          placeholder="Enter a name..."
        />
        <button type="submit">Greet</button>
      </form>
      <p>{greetMsg()}</p>

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
