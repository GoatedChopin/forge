<script lang="ts">
  import { page } from "$app/state";
</script>

<div class="error-page">
  <h1>{page.status}</h1>
  <p>{page.error?.message ?? "Page not found"}</p>
  <a href="/">Back to demo</a>
</div>

<style>
  .error-page {
    text-align: center;
    padding: 4rem 1.5rem;
    font-family: system-ui, -apple-system, sans-serif;
  }

  h1 {
    font-size: 4rem;
    margin: 0 0 0.5rem;
  }

  p {
    color: #6b7280;
    margin-bottom: 2rem;
  }

  a {
    color: #3b82f6;
    text-decoration: underline;
  }
</style>
