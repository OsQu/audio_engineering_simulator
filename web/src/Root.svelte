<script lang="ts">
  // The app root: the first thing mounted (main.ts). It owns the router and switches between the two view
  // roots — the scene view (App) and the device workbench — by URL. Each view root constructs its own
  // SceneSession, so they never share engine state; the router is the only thing above them.

  import App from "./App.svelte";
  import { Router } from "./router.svelte";
  import Workbench from "./Workbench.svelte";

  const router = new Router();

  // Browser back/forward: re-read the URL. Listener lifecycle tied to this mount.
  $effect(() => {
    const onPop = (): void => router.sync();
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  });
</script>

{#if router.route.view === "workbench"}
  <Workbench typeId={router.route.typeId} navigate={router.navigate} />
{:else}
  <App />
{/if}
