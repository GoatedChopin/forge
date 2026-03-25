<script lang="ts">
  import { ForgeProvider } from "@forge-rs/svelte";
  import { PUBLIC_API_URL } from "$env/static/public";

  interface Props {
    children: import("svelte").Snippet;
  }

  let { children }: Props = $props();
</script>

<ForgeProvider url={PUBLIC_API_URL}>
  <div class="app-layout">
    <nav>
      <a href="/" class="nav-brand">minimal</a>
      <div class="nav-links">
        <a href="/">Home</a>
        <a href="/about">About</a>
      </div>
    </nav>
    <main>
      {@render children()}
    </main>
  </div>
</ForgeProvider>

<style>
  .app-layout {
    font-family: system-ui, -apple-system, sans-serif;
    min-height: 100vh;
    display: flex;
    flex-direction: column;
  }

  nav {
    border-bottom: 1px solid #e5e7eb;
    padding: 1rem 1.5rem;
    display: flex;
    align-items: center;
    gap: 2rem;
    max-width: 56rem;
    width: 100%;
    margin: 0 auto;
    box-sizing: border-box;
  }

  .nav-brand {
    font-weight: 700;
    font-size: 1.1rem;
    text-decoration: none;
    color: inherit;
  }

  .nav-links {
    display: flex;
    gap: 1.5rem;
  }

  .nav-links a {
    text-decoration: none;
    color: #6b7280;
  }

  .nav-links a:hover {
    color: #111827;
  }

  main {
    flex: 1;
    max-width: 56rem;
    width: 100%;
    margin: 0 auto;
    padding: 2rem 1.5rem;
    line-height: 1.5;
    box-sizing: border-box;
  }
</style>
