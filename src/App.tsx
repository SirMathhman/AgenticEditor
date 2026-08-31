import { createSignal, onMount, Show } from "solid-js";
import {
  isImagePath,
  listTree,
  readFile,
  readFileData,
  type TreeNode,
} from "./lib/ipc";
import "./App.css";

function TreeItem(props: {
  node: TreeNode;
  selectedPath: string;
  onSelect: (path: string) => void;
}) {
  const [expanded, setExpanded] = createSignal(false);

  function handleClick() {
    if (props.node.is_dir) {
      setExpanded(!expanded());
    } else {
      props.onSelect(props.node.path);
    }
  }

  return (
    <li class="tree-item">
      <button
        type="button"
        class="tree-row"
        classList={{
          selected:
            !props.node.is_dir && props.selectedPath === props.node.path,
        }}
        onClick={handleClick}
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
            <TreeItem
              node={child}
              selectedPath={props.selectedPath}
              onSelect={props.onSelect}
            />
          ))}
        </ul>
      </Show>
    </li>
  );
}

function App() {
  const [tree, setTree] = createSignal<TreeNode[]>([]);
  const [treeError, setTreeError] = createSignal("");
  const [selectedPath, setSelectedPath] = createSignal("");
  const [fileContent, setFileContent] = createSignal("");
  const [imageSrc, setImageSrc] = createSignal("");
  const [fileError, setFileError] = createSignal("");

  async function loadTree() {
    try {
      setTree(await listTree());
      setTreeError("");
    } catch (err) {
      setTreeError(String(err));
    }
  }

  async function selectFile(path: string) {
    setSelectedPath(path);
    setFileError("");
    setFileContent("");
    setImageSrc("");
    try {
      if (isImagePath(path)) {
        const data = await readFileData(path);
        setImageSrc(`data:${data.mime_type};base64,${data.data}`);
      } else {
        setFileContent(await readFile(path));
      }
    } catch (err) {
      setFileError(String(err));
    }
  }

  onMount(() => {
    loadTree();
  });

  return (
    <main class="container">
      <section class="files">
        <div class="row">
          <h2>Files</h2>
          <button type="button" onClick={loadTree}>
            Refresh
          </button>
        </div>
        {treeError() ? (
          <p class="error">{treeError()}</p>
        ) : (
          <ul class="tree">
            {tree().map((node) => (
              <TreeItem
                node={node}
                selectedPath={selectedPath()}
                onSelect={selectFile}
              />
            ))}
          </ul>
        )}
      </section>

      <section class="content">
        <Show
          when={selectedPath()}
          fallback={
            <p class="content-placeholder">
              Select a file to view its contents.
            </p>
          }
        >
          <div class="row">
            <h2>{selectedPath()}</h2>
          </div>
          <Show
            when={!fileError()}
            fallback={<p class="error">{fileError()}</p>}
          >
            <Show
              when={imageSrc()}
              fallback={<pre class="content-pre">{fileContent()}</pre>}
            >
              <img class="content-image" src={imageSrc()} alt={selectedPath()} />
            </Show>
          </Show>
        </Show>
      </section>
    </main>
  );
}

export default App;
