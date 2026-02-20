<script lang="ts">
  import { listTodos$, createTodo, updateTodo, deleteTodo } from "$lib/forge";
  import type { ForgeError } from "$lib/forge";

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
  <div class="shell">
    <section class="hero">
      <p class="kicker">Todo Example</p>
      <h1>Todos</h1>
      <p class="lede">
        Real-time CRUD in one file. Every change syncs via SSE.
      </p>
    </section>

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
            {#each todos.data as todo, i (todo.id)}
              <li
                class:completed={todo.completed}
                style="animation-delay: {i * 40}ms"
              >
                <label>
                  <input
                    type="checkbox"
                    checked={todo.completed}
                    onchange={() => handleToggle(todo.id, todo.completed)}
                  />
                  <span class="check-icon">
                    {#if todo.completed}&#10003;{/if}
                  </span>
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
  @import url("https://fonts.googleapis.com/css2?family=Fraunces:opsz,wght@9..144,700;9..144,900&family=IBM+Plex+Sans:wght@400;500;600&display=swap");

  :global(body) {
    margin: 0;
    background:
      radial-gradient(
        circle at 30% 15%,
        rgba(251, 220, 159, 0.15) 0%,
        transparent 50%
      ),
      radial-gradient(
        circle at 75% 80%,
        rgba(120, 214, 192, 0.1) 0%,
        transparent 45%
      ),
      linear-gradient(155deg, #111a1f 0%, #0a1015 50%, #141e25 100%);
    color: #eaf5f0;
    font-family: "IBM Plex Sans", sans-serif;
    min-height: 100vh;
  }

  main {
    max-width: 32rem;
    margin: 0 auto;
    padding: 0 1rem;
  }

  .shell {
    padding: 2.5rem 0 3rem;
    display: grid;
    gap: 1rem;
  }

  .hero,
  .input-panel,
  .list-panel {
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 18px;
    background: rgba(11, 18, 24, 0.7);
    backdrop-filter: blur(8px);
    box-shadow: 0 14px 40px rgba(0, 0, 0, 0.3);
    padding: 1.3rem;
  }

  .hero {
    animation: rise 0.5s ease-out;
  }

  .input-panel {
    animation: rise 0.6s ease-out;
  }

  .list-panel {
    animation: rise 0.7s ease-out;
  }

  .kicker {
    margin: 0;
    font-size: 0.72rem;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: #a8d4c4;
  }

  h1 {
    margin: 0.35rem 0 0;
    font-family: "Fraunces", serif;
    font-weight: 900;
    line-height: 1.1;
    font-size: clamp(1.5rem, 4vw, 2rem);
  }

  .lede {
    margin: 0.3rem 0 0;
    color: #b0cfc4;
    font-size: 0.92rem;
  }

  .input-row {
    display: flex;
    gap: 0.5rem;
  }

  .input-row input {
    flex: 1;
    padding: 0.65rem 0.8rem;
    font-size: 0.95rem;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 12px;
    background: rgba(255, 255, 255, 0.06);
    color: #eef8f4;
    font-family: inherit;
    outline: none;
    transition: border-color 0.2s;
  }

  .input-row input:focus {
    border-color: rgba(254, 214, 139, 0.5);
  }

  .input-row button {
    padding: 0.65rem 1.2rem;
    font-size: 0.95rem;
    background: linear-gradient(96deg, #fed68b 0%, #78d6c0 100%);
    color: #0f1d23;
    border: 0;
    border-radius: 12px;
    cursor: pointer;
    font-weight: 700;
    font-family: inherit;
    transition:
      transform 0.15s ease,
      opacity 0.15s;
    white-space: nowrap;
  }

  .input-row button:hover {
    transform: translateY(-1px);
  }

  .input-row button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
    transform: none;
  }

  .list-head {
    display: flex;
    justify-content: flex-end;
    margin-bottom: 0.5rem;
  }

  .summary {
    font-size: 0.76rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: #9cc4b6;
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
    padding: 0.7rem 0.6rem;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    animation: fadeIn 0.35s ease-out both;
    transition: opacity 0.25s;
  }

  li:last-child {
    border-bottom: none;
  }

  li label {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex: 1;
    cursor: pointer;
  }

  li label input[type="checkbox"] {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
  }

  .check-icon {
    width: 1.35rem;
    height: 1.35rem;
    border: 1.5px solid rgba(255, 255, 255, 0.25);
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.75rem;
    color: #78d6c0;
    flex-shrink: 0;
    transition:
      border-color 0.2s,
      background 0.2s;
  }

  li.completed .check-icon {
    border-color: rgba(120, 214, 192, 0.4);
    background: rgba(120, 214, 192, 0.12);
  }

  .title {
    font-size: 0.95rem;
    transition:
      color 0.2s,
      opacity 0.2s;
  }

  li.completed .title {
    text-decoration: line-through;
    color: #6a8e82;
    opacity: 0.7;
  }

  .delete {
    background: none;
    border: none;
    color: rgba(255, 160, 140, 0.7);
    cursor: pointer;
    padding: 0.25rem 0.5rem;
    font-size: 0.8rem;
    font-family: inherit;
    border-radius: 6px;
    transition:
      color 0.15s,
      background 0.15s;
    opacity: 0;
  }

  li:hover .delete {
    opacity: 1;
  }

  .delete:hover {
    color: #ffa08c;
    background: rgba(255, 140, 120, 0.1);
  }

  .status {
    color: #8ab5a7;
    text-align: center;
    padding: 1.5rem 1rem;
    font-size: 0.9rem;
  }

  .error {
    color: #ffd7c7;
    padding: 0.55rem 0.7rem;
    background: rgba(188, 63, 32, 0.25);
    border: 1px solid rgba(255, 184, 161, 0.25);
    border-radius: 10px;
    margin-top: 0.7rem;
    font-size: 0.88rem;
  }

  .count {
    color: #8ab5a7;
    text-align: center;
    font-size: 0.82rem;
    margin-top: 0.6rem;
    padding-top: 0.5rem;
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(10px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
      transform: translateX(-6px);
    }
    to {
      opacity: 1;
      transform: translateX(0);
    }
  }
</style>
