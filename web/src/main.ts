// Entry point: mount the Svelte app. `Root` owns the router and picks the view root by URL — the scene
// view (App.svelte) or the device workbench (Workbench.svelte); engine bring-up and the control
// transport live in engine.ts. (The page is the throwaway harness; this infrastructure carries into
// the real UI — Story 4.2 onward.)

import { mount } from "svelte";
// Design-system tokens: puts the --ae-* custom properties on :root + registers
// the @font-face rules. Imported once, globally. The .ae-* component *recipes*
// (styles/components.css) are NOT imported — they're copied into each widget's
// scoped <style> during the re-skin; the tokens below resolve for them for free.
import "./styles/tokens.css";
// Base layer: applies the room-level tokens (dark background, base text, dark
// native controls). Must load after tokens.css so the vars exist.
import "./styles/base.css";
import Root from "./Root.svelte";

const target = document.getElementById("app");
if (!target) throw new Error("missing #app mount point");

export default mount(Root, { target });
