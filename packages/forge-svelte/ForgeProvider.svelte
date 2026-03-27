<!--
-->
<script lang="ts">
  import { onMount, onDestroy, type Snippet } from 'svelte';
  import { createForgeClient } from './client.js';
  import { setForgeClient, setAuthState } from './context.js';
  import type { AuthState, ConnectionState } from './types.js';

  interface Props {
    url: string;
    getToken?: () => string | null | Promise<string | null>;
    onAuthError?: (error: import('./types.js').ForgeError) => void;
    onConnectionChange?: (state: ConnectionState) => void;
    children: Snippet;
  }

  let { url, getToken, onAuthError, onConnectionChange, children }: Props = $props();

  // svelte-ignore state_referenced_locally -- url and getToken are stable config, not reactive state
  const client = createForgeClient({
    url,
    getToken,
    onAuthError,
  });

  setForgeClient(client);

  const authState: AuthState = $state({ user: null, token: null, loading: true });
  setAuthState(authState);

  onMount(() => {
    const unsubscribe = client.onConnectionStateChange((state) => {
      onConnectionChange?.(state);
    });

    // Connect handles token resolution internally before establishing SSE
    client.connect().then(async () => {
      if (getToken) {
        authState.token = await getToken();
      }
      authState.loading = false;
    }).catch(() => {
      authState.loading = false;
    });

    return unsubscribe;
  });

  onDestroy(() => client.disconnect());
</script>

{@render children()}
