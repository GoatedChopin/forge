<script lang="ts">
  import { listTodos$, createTodo, updateTodo, deleteTodo } from "$lib/forge";
  import type { ForgeError } from "$lib/forge";
  import { getForgeSignals } from "@forge-rs/svelte";

  const signals = getForgeSignals();
  const todos = listTodos$();

  let newTitle: string = $state("");
  let error: ForgeError | null = $state(null);
  let adding: boolean = $state(false);

  let remainingCount = $derived(
    todos.data?.filter((t) => !t.completed).length ?? 0,
  );

  async function handleAdd() {
    if (!newTitle.trim()) return;
    adding = true;
    error = null;
    signals.breadcrumb("Adding todo", { title: newTitle.trim() });
    try {
      await createTodo({ title: newTitle.trim() });
      signals.track("todo_created", { title: newTitle.trim() });
      newTitle = "";
    } catch (e) {
      error = e as ForgeError;
      signals.track("todo_create_error", { error: (e as ForgeError).message });
    } finally {
      adding = false;
    }
  }

  async function handleToggle(id: string, completed: boolean) {
    signals.track("todo_toggled", { id, completed: !completed });
    try {
      await updateTodo({ id, completed: !completed });
    } catch (e) {
      error = e as ForgeError;
      signals.track("todo_toggle_error", { error: (e as ForgeError).message });
    }
  }

  async function handleDelete(id: string) {
    signals.breadcrumb("Deleting todo", { id });
    try {
      await deleteTodo({ id });
      signals.track("todo_deleted", { id });
    } catch (e) {
      error = e as ForgeError;
      signals.track("todo_delete_error", { error: (e as ForgeError).message });
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    signals.breadcrumb("Enter key pressed in todo input");
    if (e.key === "Enter") handleAdd();
  }
</script>

<main>
  <div class="shell">
    <header class="hero">
      <h1>Todos</h1>
    </header>

    <section class="input-panel">
      <div class="input-row">
        <input
          type="text"
          placeholder="What needs to be done?"
          bind:value={newTitle}
          onkeydown={handleKeydown}
          disabled={adding}
        />
        <button onclick={handleAdd} disabled={adding || !newTitle.trim()}>
          {adding ? "Adding..." : "Add"}
        </button>
      </div>

      {#if error}
        <p class="error">{error.message}</p>
      {/if}
    </section>

    <section class="list-panel">
      {#if todos.data && todos.data.length > 0}
        <div class="list-head">
          <span class="summary">{remainingCount} remaining</span>
        </div>
      {/if}

      {#if todos.loading}
        <p class="status">Loading...</p>
      {:else if todos.error}
        <p class="error">{todos.error.message}</p>
      {:else if todos.data}
        {#if todos.data.length === 0}
          <p class="status">No todos yet. Add one above!</p>
        {:else}
          <ul>
            {#each todos.data as todo (todo.id)}
              <li class:completed={todo.completed}>
                <label>
                  <input
                    type="checkbox"
                    checked={todo.completed}
                    onchange={() => handleToggle(todo.id, todo.completed)}
                  />
                  <span class="title">{todo.title}</span>
                </label>
                <button class="delete" onclick={() => handleDelete(todo.id)}>
                  Delete
                </button>
              </li>
            {/each}
          </ul>
          <p class="count">
            {remainingCount} remaining
          </p>
        {/if}
      {/if}
    </section>
  </div>
</main>

<style>
  :global(body) {
    margin: 0;
    background: #fff;
    color: #222;
    font-family:
      -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    min-height: 100vh;
  }

  main {
    max-width: 480px;
    margin: 0 auto;
    padding: 0 1rem;
  }

  .shell {
    padding: 2rem 0 3rem;
  }

  .hero {
    padding: 0 0 0.75rem;
    border-bottom: 1px solid #e5e5e5;
    margin-bottom: 1rem;
  }

  h1 {
    margin: 0;
    font-weight: 600;
    font-size: 1.5rem;
    color: #111;
  }

  .input-panel {
    margin-bottom: 1.5rem;
  }

  .input-row {
    display: flex;
    gap: 0.5rem;
  }

  .input-row input {
    flex: 1;
    padding: 0.5rem 0.75rem;
    font-size: 0.9rem;
    border: 1px solid #ccc;
    border-radius: 4px;
    background: #fff;
    color: #222;
    font-family: inherit;
    outline: none;
  }

  .input-row input:focus {
    border-color: #888;
  }

  .input-row button {
    padding: 0.5rem 1rem;
    font-size: 0.9rem;
    background: #111;
    color: #fff;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-weight: 500;
    font-family: inherit;
    white-space: nowrap;
  }

  .input-row button:hover {
    background: #333;
  }

  .input-row button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .list-head {
    display: flex;
    justify-content: flex-end;
    margin-bottom: 0.5rem;
  }

  .summary {
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #888;
  }

  ul {
    list-style: none;
    padding: 0;
    margin: 0;
  }

  li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.6rem 0;
    border-bottom: 1px solid #eee;
  }

  li:last-child {
    border-bottom: none;
  }

  li label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex: 1;
    cursor: pointer;
  }

  li label input[type="checkbox"] {
    width: 1rem;
    height: 1rem;
    margin: 0;
    cursor: pointer;
    flex-shrink: 0;
    accent-color: #111;
  }

  .title {
    font-size: 0.9rem;
  }

  li.completed .title {
    text-decoration: line-through;
    color: #999;
  }

  .delete {
    background: none;
    border: 1px solid transparent;
    color: #c44;
    cursor: pointer;
    padding: 0.2rem 0.5rem;
    font-size: 0.78rem;
    font-family: inherit;
    border-radius: 3px;
    opacity: 0;
  }

  li:hover .delete {
    opacity: 1;
  }

  .delete:hover {
    background: #fef0f0;
    border-color: #e8c0c0;
  }

  .status {
    color: #888;
    text-align: center;
    padding: 1.5rem 1rem;
    font-size: 0.88rem;
  }

  .error {
    color: #b91c1c;
    padding: 0.5rem 0.7rem;
    background: #fef2f2;
    border: 1px solid #fecaca;
    border-radius: 4px;
    margin-top: 0.5rem;
    font-size: 0.85rem;
  }

  .count {
    color: #888;
    text-align: center;
    font-size: 0.8rem;
    margin-top: 0.5rem;
    padding-top: 0.5rem;
  }
</style>
