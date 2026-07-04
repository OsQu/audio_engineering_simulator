<script lang="ts">
  // The device workbench — the second view root (Epic 6). A focused single-device development view at
  // /devices/<typeId>. This is the Task-6.2.1 stub: it only proves the routing switches here and can
  // navigate back. The suspended-context boot + catalog resolution (6.2.3) and the bench stage (6.2.4)
  // land next; for now it just echoes the route.

  interface Props {
    /** The requested device type id from the URL, or "" for the bare /devices index. */
    typeId: string;
    /** Route to a new path (History API push), shared from the router. */
    navigate: (path: string) => void;
  }

  let { typeId, navigate }: Props = $props();
</script>

<main class="workbench">
  <p>Device workbench (stub) — <strong>{typeId || "(index)"}</strong></p>
  <button type="button" onclick={() => navigate("/")}>← scene view</button>
</main>

<style>
  .workbench {
    padding: 2rem;
    font: 15px/1.5 var(--ae-font-ui);
    color: var(--ae-text-secondary);
  }
</style>
