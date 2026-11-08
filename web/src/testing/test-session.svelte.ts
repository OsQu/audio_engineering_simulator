// Browser-fixture session: it exercises the production App/WorldView/PatchController composition without
// creating an AudioContext or worklet. The real-worklet boundary is covered
// by a separate browser smoke, not this fixture.

import type { Scene } from "../scene-store";
import { SceneSession } from "../session.svelte";
import type { BrowserFixture } from "./browser-fixtures";

export class TestSceneSession extends SceneSession {
  readonly submissions: Scene[] = [];

  constructor(fixture: BrowserFixture) {
    super(fixture.scene);
    this.catalog = fixture.catalog;
    this.cables = fixture.cables;
    this.ready = true;
    this.status = `browser fixture: ${fixture.id}`;
  }

  override submitPatchCandidate(candidate: Scene): boolean {
    const accepted = structuredClone($state.snapshot(candidate));
    this.submissions.push(accepted);
    this.scene = accepted;
    this.status = "fixture patch accepted";
    return true;
  }

  override proposeTopology(label: string, edit: (scene: Scene) => void): boolean {
    // Snapshot first, as production does: the scene is a $state proxy, which structuredClone rejects.
    const accepted = structuredClone($state.snapshot(this.scene));
    edit(accepted);
    this.submissions.push(accepted);
    this.scene = accepted;
    this.status = `fixture topology accepted: ${label}`;
    return true;
  }
}
