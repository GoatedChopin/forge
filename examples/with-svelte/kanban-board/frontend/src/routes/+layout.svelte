<script lang="ts">
  import { ForgeProvider, auth } from "$lib/forge";
  import { PUBLIC_API_URL } from "$env/static/public";
  import { onMount } from "svelte";

  interface Props {
    children: import("svelte").Snippet;
  }

  let { children }: Props = $props();

  function getToken() {
    return auth.token;
  }

  onMount(() => {
    auth.startRefreshLoop(PUBLIC_API_URL);
    return () => auth.stopRefreshLoop();
  });
</script>

<ForgeProvider url={PUBLIC_API_URL} {getToken}>
  {@render children()}
</ForgeProvider>
