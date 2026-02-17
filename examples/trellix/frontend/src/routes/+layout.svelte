<script lang="ts">
  import { ForgeProvider } from "$lib/forge";
  import { PUBLIC_API_URL } from "$env/static/public";

  interface Props {
    children: import("svelte").Snippet;
  }

  let { children }: Props = $props();

  function getToken() {
    return localStorage.getItem("trellix_token");
  }
</script>

<ForgeProvider url={PUBLIC_API_URL} {getToken}>
  {@render children()}
</ForgeProvider>
