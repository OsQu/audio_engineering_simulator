// Entry point: mount the Svelte app. All UI lives in App.svelte; engine bring-up and the control
// transport live in engine.ts. (The page is the throwaway harness; this infrastructure carries into
// the real UI — Story 4.2 onward.)

import { mount } from "svelte";
import App from "./App.svelte";

const target = document.getElementById("app");
if (!target) throw new Error("missing #app mount point");

export default mount(App, { target });
