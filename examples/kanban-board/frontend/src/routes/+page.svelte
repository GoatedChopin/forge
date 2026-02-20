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
    <section class="brand">
      <p class="kicker">Kanban Board Example</p>
      <h1>Kanban Board</h1>
      <p class="lede">
        Auth, projects, tasks, jobs, workflows, cron. All from one Rust binary.
      </p>
    </section>

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
  @import url("https://fonts.googleapis.com/css2?family=Fraunces:opsz,wght@9..144,700;9..144,900&family=IBM+Plex+Sans:wght@400;500;600&display=swap");

  :global(body) {
    margin: 0;
    background:
      radial-gradient(
        circle at 20% 25%,
        rgba(251, 220, 159, 0.12) 0%,
        transparent 50%
      ),
      radial-gradient(
        circle at 80% 75%,
        rgba(120, 214, 192, 0.08) 0%,
        transparent 45%
      ),
      linear-gradient(155deg, #111a1f 0%, #0a1015 50%, #141e25 100%);
    color: #eaf5f0;
    font-family: "IBM Plex Sans", sans-serif;
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
    width: min(420px, 100%);
    display: grid;
    gap: 1rem;
  }

  .brand,
  .form-panel {
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 18px;
    background: rgba(11, 18, 24, 0.7);
    backdrop-filter: blur(8px);
    box-shadow: 0 14px 40px rgba(0, 0, 0, 0.3);
    padding: 1.3rem;
  }

  .brand {
    animation: rise 0.5s ease-out;
    text-align: center;
  }

  .form-panel {
    animation: rise 0.65s ease-out;
  }

  .kicker {
    margin: 0;
    font-size: 0.72rem;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: #a8d4c4;
  }

  h1 {
    margin: 0.3rem 0 0;
    font-family: "Fraunces", serif;
    font-weight: 900;
    font-size: 2rem;
  }

  .lede {
    margin: 0.3rem 0 0;
    color: #b0cfc4;
    font-size: 0.88rem;
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
  }

  h2 {
    margin: 0;
    font-family: "Fraunces", serif;
    font-size: 1.25rem;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    font-size: 0.82rem;
    color: #b8d8cc;
  }

  input {
    padding: 0.6rem 0.75rem;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.06);
    color: #eef8f4;
    font: inherit;
    font-size: 0.95rem;
    outline: none;
    transition: border-color 0.2s;
  }

  input:focus {
    border-color: rgba(254, 214, 139, 0.45);
  }

  button[type="submit"] {
    padding: 0.7rem;
    background: linear-gradient(96deg, #fed68b 0%, #78d6c0 100%);
    color: #0f1d23;
    border: 0;
    border-radius: 12px;
    font-size: 0.95rem;
    font-weight: 700;
    cursor: pointer;
    font-family: inherit;
    transition: transform 0.15s ease;
  }

  button[type="submit"]:hover {
    transform: translateY(-1px);
  }

  button[type="submit"]:disabled {
    opacity: 0.5;
    cursor: not-allowed;
    transform: none;
  }

  .error {
    color: #ffd7c7;
    font-size: 0.88rem;
    margin: 0;
    padding: 0.55rem 0.7rem;
    background: rgba(188, 63, 32, 0.25);
    border: 1px solid rgba(255, 184, 161, 0.25);
    border-radius: 10px;
  }

  .toggle {
    text-align: center;
    font-size: 0.85rem;
    color: #8ab5a7;
    margin: 0;
  }

  .toggle button {
    background: none;
    border: none;
    color: #fed68b;
    cursor: pointer;
    text-decoration: underline;
    font-size: 0.85rem;
    font-family: inherit;
    padding: 0;
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
</style>
