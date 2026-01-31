<script lang="ts">
  import { listTodos$, createTodo, updateTodo, deleteTodo } from "$lib/forge";
  import type { ForgeError } from "$lib/forge";

  const todos = listTodos$();

  let newTitle: string = $state("");
  let error: ForgeError | null = $state(null);
  let adding: boolean = $state(false);

  async function handleAdd() {
    if (!newTitle.trim()) return;
    adding = true;
    error = null;
    try {
      await createTodo({ title: newTitle.trim() });
      newTitle = "";
    } catch (e) {
      error = e as ForgeError;
    } finally {
      adding = false;
    }
  }

  async function handleToggle(id: string, completed: boolean) {
    try {
      await updateTodo({ id, completed: !completed });
    } catch (e) {
      error = e as ForgeError;
    }
  }

  async function handleDelete(id: string) {
    try {
      await deleteTodo({ id });
    } catch (e) {
      error = e as ForgeError;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") handleAdd();
  }
</script>

<main>
  <h1>Todos</h1>

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
        {todos.data.filter((t) => !t.completed).length} remaining
      </p>
    {/if}
  {/if}
</main>

<style>
  main {
    max-width: 32rem;
    margin: 2rem auto;
    padding: 0 1rem;
    font-family: system-ui, -apple-system, sans-serif;
  }

  h1 {
    margin: 0 0 1rem;
  }

  .input-row {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 1rem;
  }

  .input-row input {
    flex: 1;
    padding: 0.5rem;
    font-size: 1rem;
    border: 1px solid #ccc;
    border-radius: 4px;
  }

  .input-row button {
    padding: 0.5rem 1rem;
    font-size: 1rem;
    background: #0066cc;
    color: white;
    border: none;
    border-radius: 4px;
    cursor: pointer;
  }

  .input-row button:disabled {
    background: #999;
    cursor: not-allowed;
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
    padding: 0.75rem;
    border-bottom: 1px solid #eee;
  }

  li label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex: 1;
    cursor: pointer;
  }

  li.completed .title {
    text-decoration: line-through;
    color: #999;
  }

  .delete {
    background: none;
    border: none;
    color: #cc0000;
    cursor: pointer;
    padding: 0.25rem 0.5rem;
    font-size: 0.875rem;
  }

  .delete:hover {
    text-decoration: underline;
  }

  .status {
    color: #666;
    text-align: center;
    padding: 1rem;
  }

  .error {
    color: #cc0000;
    padding: 0.5rem;
    background: #fff0f0;
    border-radius: 4px;
    margin-bottom: 1rem;
  }

  .count {
    color: #666;
    text-align: center;
    font-size: 0.875rem;
    margin-top: 1rem;
  }
</style>
