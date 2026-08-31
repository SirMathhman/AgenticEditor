import { createSignal, onMount } from "solid-js";
import logo from "./assets/logo.svg";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

function App() {
  const [greetMsg, setGreetMsg] = createSignal("");
  const [name, setName] = createSignal("");
  const [files, setFiles] = createSignal<string[]>([]);
  const [filesError, setFilesError] = createSignal("");

  async function greet() {
    // Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
    setGreetMsg(await invoke("greet", { name: name() }));
  }

  async function loadFiles() {
    try {
      setFiles(await invoke<string[]>("list_files"));
      setFilesError("");
    } catch (err) {
      setFilesError(String(err));
    }
  }

  onMount(() => {
    loadFiles();
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
          <button type="button" onClick={loadFiles}>
            Refresh
          </button>
        </div>
        {filesError() ? (
          <p class="error">{filesError()}</p>
        ) : (
          <ul>
            {files().map((file) => (
              <li>{file}</li>
            ))}
          </ul>
        )}
      </section>
    </main>
  );
}

export default App;
