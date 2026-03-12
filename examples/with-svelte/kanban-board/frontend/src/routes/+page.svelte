<script lang="ts">
  import { goto } from "$app/navigation";
  import { resolve } from "$app/paths";
  import { getForgeClient, register, login } from "$lib/forge";
  import type { ForgeError } from "$lib/forge";

  let mode: "login" | "register" = $state("login");
  let email = $state("");
  let name = $state("");
  let password = $state("");
  let error: string | null = $state(null);
  let loading = $state(false);

  async function handleSubmit() {
    error = null;
    loading = true;

    try {
      // Clear stale tokens so the auth middleware doesn't reject public endpoints
      localStorage.removeItem("kanban_token");
      localStorage.removeItem("kanban_user");

      const result =
        mode === "register"
          ? await register({ email, name, password })
          : await login({ email, password });

      localStorage.setItem("kanban_token", result.token);
      localStorage.setItem("kanban_user", JSON.stringify(result.user));
      await getForgeClient().reconnect();
      goto(resolve("/app"));
    } catch (e) {
      error = (e as ForgeError).message ?? "Something went wrong";
    } finally {
      loading = false;
    }
  }

  function toggleMode() {
    mode = mode === "login" ? "register" : "login";
    error = null;
  }
</script>

<main>
  <div class="auth-shell">
    <header class="brand">
      <h1>Kanban Board</h1>
    </header>

    <section class="form-panel">
      <form
        onsubmit={(event) => {
          event.preventDefault();
          void handleSubmit();
        }}
      >
        <h2>{mode === "login" ? "Sign in" : "Create account"}</h2>

        {#if error}
          <p class="error">{error}</p>
        {/if}

        {#if mode === "register"}
          <label>
            Name
            <input type="text" bind:value={name} required disabled={loading} />
          </label>
        {/if}

        <label>
          Email
          <input type="email" bind:value={email} required disabled={loading} />
        </label>

        <label>
          Password
          <input
            type="password"
            bind:value={password}
            required
            minlength="8"
            disabled={loading}
          />
        </label>

        <button type="submit" disabled={loading}>
          {loading ? "..." : mode === "login" ? "Sign in" : "Create account"}
        </button>

        <p class="toggle">
          {mode === "login" ? "No account?" : "Already have an account?"}
          <button type="button" onclick={toggleMode}>
            {mode === "login" ? "Register" : "Sign in"}
          </button>
        </p>
      </form>
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
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 100vh;
    padding: 2rem 1rem;
  }

  .auth-shell {
    width: min(380px, 100%);
    display: grid;
    gap: 1.5rem;
  }

  .brand {
    text-align: center;
  }

  h1 {
    margin: 0;
    font-weight: 600;
    font-size: 1.5rem;
    color: #111;
  }

  .form-panel {
    border: 1px solid #e5e5e5;
    border-radius: 6px;
    padding: 1.5rem;
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  h2 {
    margin: 0;
    font-weight: 600;
    font-size: 1.1rem;
    color: #111;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.82rem;
    color: #555;
  }

  input {
    padding: 0.5rem 0.75rem;
    border: 1px solid #ccc;
    border-radius: 4px;
    background: #fff;
    color: #222;
    font: inherit;
    font-size: 0.9rem;
    outline: none;
  }

  input:focus {
    border-color: #888;
  }

  button[type="submit"] {
    padding: 0.6rem;
    background: #111;
    color: #fff;
    border: none;
    border-radius: 4px;
    font-size: 0.9rem;
    font-weight: 500;
    cursor: pointer;
    font-family: inherit;
  }

  button[type="submit"]:hover {
    background: #333;
  }

  button[type="submit"]:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .error {
    color: #b91c1c;
    font-size: 0.85rem;
    margin: 0;
    padding: 0.5rem 0.7rem;
    background: #fef2f2;
    border: 1px solid #fecaca;
    border-radius: 4px;
  }

  .toggle {
    text-align: center;
    font-size: 0.85rem;
    color: #666;
    margin: 0;
  }

  .toggle button {
    background: none;
    border: none;
    color: #111;
    cursor: pointer;
    text-decoration: underline;
    font-size: 0.85rem;
    font-family: inherit;
    padding: 0;
  }
</style>
