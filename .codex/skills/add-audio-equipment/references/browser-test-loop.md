# Browser test loop

Use this loop while developing a device faceplate in the workbench. Its purpose is repeatable visual and
interaction evidence, not exhaustive browser automation.

## Choose the browser path

Prefer the first available path:

1. The built-in browser, invoked as `@Browser`.
2. Computer Use or the browser extension with an allowed Chromium browser.
3. Fixed-view screenshots supplied by the user, with the user performing interactions.

The built-in browser is not available in every Codex surface. Detect capability from the tools actually
available; do not repeatedly attempt unavailable browser controls. Do not add Playwright, another browser
dependency, or repo automation merely to compensate unless the user approves that separate scope.

## Start or verify the development loop

Use the repository commands in `CLAUDE.md`. Confirm both the WASM watcher/build and Vite server are healthy
before opening the page. Preserve any development process the user already has running rather than
starting a competing server.

Open the exact local route:

```text
http://localhost:5173/devices/<typeId>
```

Allow the local origin if the browser requests permission. The workbench boots audio suspended, so click
once in the page after every full reload to satisfy browser autoplay policy.

## Run the capability smoke test

Run this once per browser environment, and again only after browser/runtime changes or a failed reload:

1. Open `/devices/synth_voice` or another known-good catalog device.
2. Confirm the booting state clears and the device catalog/workbench renders.
3. In network or developer tools when available, confirm the WASM request succeeds.
4. Check the console for WASM instantiation, AudioWorklet, CSP, MIME-type, or cross-origin errors.
5. Click once to unlock audio and confirm the session does not report an audio/worklet fault.
6. Move one control and confirm its displayed value and rendered state respond.
7. Exercise one known-good audible patch when audio verification is in scope.
8. After the next intentional Rust/WASM rebuild, confirm reload succeeds and the URL-persisted bench scene
   restores; click once again to unlock audio.

Record each item as observed, failed, or unverified. Ordinary WASM success does not prove AudioWorklet,
audio output, Web MIDI, or hardware permissions. If the built-in browser renders the page but blocks a
required capability, try the connected Chromium path. If that also fails, stop browser-dependent work,
report the exact failing boundary, and use screenshots plus user-operated interaction for the visual gate.

Use full browser developer/CDP access only when console, network, DOM/style, or performance inspection is
needed. Treat it as elevated access and keep it limited to the local development origin.

## Establish a repeatable review state

Before comparing the faceplate, record:

- Browser viewport width and height.
- Browser zoom, normally 100%.
- Workbench camera zoom.
- Device position and front/rear orientation.
- Device config and parameter values that affect appearance.
- Supporting devices, connections, and monitored output.
- The reference image or dimensional source used for this pass.

Use the URL-persisted workbench scene to return to the same state after reload. Do not compare screenshots
taken at materially different zooms or states as if they showed a design change.

## Iterate one visual layer at a time

For each pass:

1. Choose one review layer: silhouette, grouping, physical proportions, styling, or interaction/motion.
2. Inspect the current front and rear at the recorded state.
3. Capture screenshots or leave element/area comments that identify concrete mismatches.
4. Make only the changes needed for that layer.
5. Reload, click once to unlock audio, and restore the recorded state.
6. Recheck the changed area and nearby controls for clipping, overlap, unreadable labels, and lost jack
   anchors.
7. Exercise any controls, configs, meters, face flips, focus surfaces, or patch points touched by the
   change.
8. Record what improved and what remains intentionally deferred.

Do not combine a broad visual restyle with DSP changes in the same review pass. Prefer browser comments on
specific elements over vague instructions such as “make it more realistic.”

## Visual approval gate

Present consistent front and rear views and summarize:

- The reference characteristics now represented.
- Intentional departures that keep the device original.
- Any browser capabilities that remained unverified.
- Remaining visual or interaction gaps.

Stop for explicit user approval. Detailed DSP work begins only after approval, although fixes required to
make the thin model render and interact honestly remain part of the visual slice.
