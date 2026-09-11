<script lang="ts">
  import App from "../App.svelte";
  import { browserFixture } from "./browser-fixtures";
  import { TestSceneSession } from "./test-session.svelte";

  interface Props {
    fixtureId: string;
  }

  let { fixtureId }: Props = $props();
  // svelte-ignore state_referenced_locally
  const fixture = browserFixture(fixtureId);
  const session = fixture ? new TestSceneSession(fixture) : undefined;
</script>

{#if fixture && session}
  <div data-testid="browser-fixture" data-fixture={fixture.id}>
    <App {session} />
  </div>
{:else}
  <main data-testid="browser-fixture-error">Unknown browser fixture: {fixtureId}</main>
{/if}
