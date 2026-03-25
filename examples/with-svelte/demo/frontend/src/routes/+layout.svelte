<script lang="ts">
  import { ForgeProvider } from "@forge-rs/svelte";
  import { PUBLIC_API_URL } from "$env/static/public";

  interface Props {
    children: import("svelte").Snippet;
  }

  let { children }: Props = $props();
</script>

<ForgeProvider url={PUBLIC_API_URL}>
  <nav class="app-nav">
    <a href="/" class="nav-brand">Forge Demo</a>
  </nav>
  {@render children()}
</ForgeProvider>

<style>
  .app-nav {
    max-width: 80rem;
    margin: 0 auto;
    padding: 1rem 2rem;
    display: flex;
    align-items: center;
    gap: 2rem;
  }

  .nav-brand {
    font-weight: 700;
    font-size: 1.1rem;
    text-decoration: none;
    color: inherit;
  }
</style>
