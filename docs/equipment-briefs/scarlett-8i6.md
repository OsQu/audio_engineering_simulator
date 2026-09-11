# Scarlett 8i6 equipment brief

## Inspiration and boundary

- Reference device and revision: Focusrite Scarlett 8i6, third generation.
- Simulator device: existing `Scarlett 8i6` / `scarlett_8i6` catalog entry.
- Increment boundary: add the two input-gain level halos; preserve the existing chassis, controls,
  signal model, exposed ids, and fictionalized faceplate treatment.

## Source and verified behavior

- Focusrite's third-generation 8i6 user guide identifies tri-colour LED rings around Gain 1 and Gain 2.
  Green means signal present at -24 dBFS, amber means near clipping at -6 dBFS, and red means digital
  clipping at 0 dBFS:
  <https://fael-downloads-prod.focusrite.com/customer/prod/s3fs-public/downloads/Scarlett%208i6%203rd%20Gen%20User%20Guide%20V2.pdf>
- The same manual's AIR controls, USB-C connector, and 210 × 47.5 × 149.5 mm dimensions align with the
  existing modeled face. Older source comments calling it second generation are a non-blocking
  documentation inconsistency outside this increment.

## Approved increment

### Must look right now

- One complete illuminated halo around each existing 14 mm preamp gain control.
- Off, green, amber, and red states that remain legible at the workbench's physical scale.
- The halo must not obscure the knob pointer/ticks or change the control's interaction target.

### Must minimally work now

- Gain 1 uses exposed readout `1` (In 1 block peak, dBu); Gain 2 uses readout `3` (In 2 block peak,
  dBu). The slower VU readouts `0` and `2` remain available to the debug surface.
- Convert the analog peak to the modeled ADC scale. Its 1 V-peak reference is +2.22 dBu at 0 dBFS, so
  halo thresholds in the existing dBu readout are approximately -21.78 dBu green, -3.78 dBu amber,
  and +2.22 dBu red.
- Below the green threshold the ring is visibly off. The device's existing power gate naturally sends
  silence to the meter when powered off.

### Explicitly deferred

- Peak hold, clip latching, configurable halo colours, segment/progress metering, or new meter
  ballistics.
- Corrections to older generation labels in unrelated landed comments.
- Any engine, catalog, protocol, or scene-schema change.

## Code and architecture fit

- Existing seam: `DeviceHandle.reading(id)` supplies the live readouts already exposed by the catalog.
- UI design: a device-local peak-to-state helper at `web/src/widgets/Scarlett8i6/gain-halo.ts` plus
  decorative wrappers around the two existing `Control` instances in `Scarlett8i6.svelte`; no new
  reusable widget is justified for one faceplate.
- Guardrails: focused unit coverage for threshold boundaries plus existing faceplate coverage,
  formatting, typecheck, and web tests.
- Architecture fit: additive through existing seams. Shared contracts and scene schema remain unchanged.

## Workbench acceptance

- Route: `/devices/scarlett_8i6`, fixed front and rear views at recorded viewport/browser/workbench zoom.
- Confirm both halos render without layout regression, reflect live input level independently, and do
  not block either gain control.
- Manager grade must reach 4/5 before user visual review. User approval remains required to close the
  visual task.
- User approved the rendered halos on 2026-09-11, with the helper colocated under the device folder.
