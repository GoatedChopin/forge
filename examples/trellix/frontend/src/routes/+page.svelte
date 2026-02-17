<script lang="ts">
  import { goto } from "$app/navigation";
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
      const result =
        mode === "register"
          ? await register({ email, name, password })
          : await login({ email, password });

      localStorage.setItem("trellix_token", result.token);
      localStorage.setItem("trellix_user", JSON.stringify(result.user));
      await getForgeClient().reconnect();
      goto("/app");
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
  <h1>Trellix</h1>
  <p class="subtitle">Project management, powered by Forge</p>

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
</main>

<style>
  main {
    max-width: 400px;
    margin: 4rem auto;
    padding: 0 1rem;
    font-family: system-ui, sans-serif;
  }

  h1 {
    font-size: 2rem;
    margin-bottom: 0.25rem;
  }

  .subtitle {
    color: #666;
    margin-bottom: 2rem;
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 1.5rem;
    border: 1px solid #ddd;
    border-radius: 8px;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.875rem;
    font-weight: 500;
  }

  input {
    padding: 0.5rem;
    border: 1px solid #ccc;
    border-radius: 4px;
    font-size: 1rem;
  }

  button[type="submit"] {
    padding: 0.625rem;
    background: #2563eb;
    color: white;
    border: none;
    border-radius: 4px;
    font-size: 1rem;
    cursor: pointer;
  }

  button[type="submit"]:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .error {
    color: #dc2626;
    font-size: 0.875rem;
    margin: 0;
    padding: 0.5rem;
    background: #fef2f2;
    border-radius: 4px;
  }

  .toggle {
    text-align: center;
    font-size: 0.875rem;
    color: #666;
  }

  .toggle button {
    background: none;
    border: none;
    color: #2563eb;
    cursor: pointer;
    text-decoration: underline;
    font-size: 0.875rem;
  }
</style>
