<script lang="ts">
  import { ForgeProvider, getForgeSignals } from "@forge-rs/svelte";
  import { PUBLIC_API_URL } from "$env/static/public";
  import { getToken, auth } from "$lib/forge/auth.svelte";
  import { onMount } from "svelte";

  interface Props {
    children: import("svelte").Snippet;
  }

  let { children }: Props = $props();
  let lastIdentifiedUserId: string | null = null;

  onMount(() => {
    auth.startRefreshLoop(PUBLIC_API_URL);
    window.forgeSignals = getForgeSignals();
  });

  $effect(() => {
    const user = auth.user;
    if (user && user.id !== lastIdentifiedUserId) {
      lastIdentifiedUserId = user.id;
      getForgeSignals().identify(user.id, { email: user.email, name: user.name });
    }
  });
</script>

<ForgeProvider url={PUBLIC_API_URL} {getToken} onAuthError={() => auth.handleAuthError()}>
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
