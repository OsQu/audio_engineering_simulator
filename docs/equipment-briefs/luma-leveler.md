# Luma Leveler equipment brief

Status: approved for program-design planning on 2026-09-09. This document records the researched
look-and-feel scope only; it does not approve an implementation design or detailed DSP.

## Inspiration and boundary

- **Reference device or archetype:** Teletronix LA-2A-style electro-optical leveling amplifier.
- **Exact revision/era:** the current Universal Audio hardware reissue, visually rooted in the
  late-1960s silver-face LA-2A. The 1962 gray-face revision is not the primary visual reference.
- **Original simulator name and `typeId`:** **Luma Leveler**, `luma_leveler`.
- **What may resemble the reference:** the 3U silhouette, sparse two-knob workflow, large central
  multifunction VU meter, widely spaced toggle/selector controls, rear XLR line I/O, and the broad
  warm-silver/cream/black material hierarchy.
- **What must remain original:** name, marks, logo, exact typography, legend wording and placement,
  exact control proportions, decorative line work, serial plates, and other trade-dress details.

## Sources

| Source | What it establishes | Confidence |
| --- | --- | --- |
| [Universal Audio product page](https://www.uaudio.com/products/teletronix-la-2a-classic-leveling-amplifier) | Current reissue, specifications, connection inventory, 0–40 dB limiting, 40 dB gain, and balanced stereo interconnection | verified |
| [Universal Audio hardware support page](https://help.uaudio.com/hc/en-us/articles/206356233-Teletronix-LA-2A-Classic-Leveling-Amplifier) | 19-inch/3U form, impedance options, XLR plus Jones barrier connections, meter modes, attack/release figures, power, weight | verified |
| [Official LA-2A user manual (PDF)](https://media.uaudio.com/assetlibrary/l/a/la-2a_manual.pdf) | 19 × 5.25-inch panel, 7.25-inch depth behind panel, control operation, 600-ohm balanced line faces, XLR/barrier wiring, meter modes, calibration controls, and chassis-component layout | verified for the documented reissue; some published current-page impedance figures differ |
| [Universal Audio history](https://help.uaudio.com/hc/en-us/articles/215779663-A-History-of-the-Teletronix-LA-2A-Leveling-Amplifier) | 1962 gray-face origin, 1967 silver-face change, T4 optical principle, two-stage program-dependent release, and revision history | verified |
| [Universal Audio silver-era overview](https://www.uaudio.com/blogs/ua/uad-spotlight-ua-1176-la-2a) | Late-1960s Silver model's brushed-aluminum panel and broad operating character | verified |
| [Universal Audio tube-replacement guide](https://help.uaudio.com/hc/en-us/articles/215479643-Replacing-Tubes-in-Your-UA-Analog-Hardware) | Rear/exposed tube locations and maintenance-side physical character | verified |

### Recorded source conflict

The 2000-era official manual describes a 600-ohm balanced input and output. The current UA product and
support pages list 50, 150, 250, and 600-ohm input/output settings or taps. The first simulator increment
will use fixed 600-ohm balanced faces, matching the manual's ordinary XLR operating path; selectable
impedance taps are explicitly deferred.

## Physical identity

- **Form factor and dimensions:** 19-inch, 3U rackmount. Author at 482.6 mm wide × 133.35 mm high ×
  approximately 184.15 mm deep (7.25 inches behind the panel).
- **Front/rear silhouette:** a tall, unusually sparse front plate within a dark rack chassis. The rear
  should read as older tube equipment rather than a dense modern processor: widely separated line I/O
  and simplified silhouettes suggesting the externally accessible tubes and transformers.
- **Dominant visual landmarks:** two oversized dark knobs and one large, cream-faced moving-coil meter
  in a dark bezel.
- **Control groups, left to right:** Compress/Limit toggle; large output Gain knob; central VU meter;
  large Peak Reduction knob; three-position meter selector above a Power toggle.
- **Meter/display type and behavior:** one physical needle. The selector chooses gain reduction, output
  referenced to +4 dBu, or output referenced to +10 dBu. Gain-reduction indication returns toward zero
  with the detector rather than acting as a generic output meter.
- **Materials, palette, and typography character:** brushed warm-silver metal, cream meter card, black
  knobs and switch hardware, dark engraved/silkscreen-style labeling, restrained red meter accents.
  Use original typography and decoration.
- **Front I/O:** none.
- **Rear I/O:** one functional balanced XLR line input and one functional balanced XLR line output in
  the first increment.
- **Details that can be omitted without losing the identity:** exact badges and certification marks,
  serial plate, fasteners, fold-down-panel mechanics, internal labeling, component designators, exact
  tube/transformer models, and minor calibration legends.

## Operating identity

- **Intended signal domain and channel shape:** one mono analog voltage channel. This is a hardware
  line processor, not a digital plug-in and not an implicit AD/DSP/DA chain.
- **Balanced/unbalanced and connector behavior:** balanced XLR input and output. The proposed first
  model uses fixed 600-ohm faces. The engine node must detect the differential input before nonlinear
  processing and apply identical gain to the output legs so common-mode behavior remains honest.
- **Primary user workflow:** raise Peak Reduction until the meter shows the desired leveling, then set
  Gain to restore the output level. Choose Compress for gentle leveling or Limit for firmer control.
- **What the learner should hear:** slower, softer gain control than a fast peak limiter, with output
  gain independent of compression amount.
- **What the learner should see:** the needle follows either gain reduction or calibrated output level,
  as chosen by the meter selector.
- **What the learner should understand:** the sparse controls do not imply a generic compressor;
  attack/release are fixed by the optical behavior, Peak Reduction acts like a threshold/drive control,
  and Gain is a separate post-reduction amplifier.
- **Bypass, power, sidechain, or routing semantics:** there is no front-panel bypass. Power off should
  silence the active output through a short de-click glide. Sidechain pre-emphasis and stereo linking
  are not exposed in the first increment.

## First increment

### Must look right now

- Correct 3U silhouette and real-mm scale.
- Sparse two-knob hierarchy, large central meter, and generous whitespace.
- Two toggle controls and a clearly three-position meter selector.
- Original warm-silver/cream/black palette with fictional Luma Leveler branding.
- Front panel without audio sockets.
- Rear functional XLR input/output, dark chassis treatment, and simplified noninteractive
  tube/transformer silhouettes.

### Must minimally work now

- One balanced analog XLR input and output in the voltage domain.
- Peak Reduction drives a basic soft-knee downward-compression curve over a useful 0–40 dB reduction
  range. Its 0–100 face legend is arbitrary rather than a claimed dB calibration.
- Gain provides up to 40 dB of output/makeup gain and does not change the compression amount.
- Compress and Limit select meaningfully different ratios (approximately 3:1 versus a high limiting
  ratio); the initial curve may be simpler than the reference's nonlinear, frequency-dependent ratio.
- Power is a smoothed output gate.
- The meter selector switches one live needle among gain reduction, +4 dBu-referenced output, and
  +10 dBu-referenced output.
- Fixed approximately 10 ms attack and a deliberately simplified release provide honest first-pass
  optical-style behavior. The full program-dependent tail is deferred.

### Explicitly deferred

- The T4-style multi-stage, history-dependent release: approximately 60 ms for the first 50%, then a
  program-dependent one-to-fifteen-second tail.
- Frequency/program-dependent ratio and the rear R37 sidechain-emphasis trimmer.
- Tube and transformer coloration, bandwidth shaping, hum/noise, soft saturation, and warm-up behavior.
- Stereo link, stereo-balance adjustment, barrier-strip terminals, selectable impedance taps, and
  meter-zero calibration.
- Detailed rear maintenance controls, exact external components, component-level circuit simulation,
  and exact trade dress.

## Code fit

- **Existing nodes to compose:** the analog metering/readout ideas in `VuMeter` are reusable, but its
  current inline single-reading presentation does not by itself provide gain reduction or selector
  semantics. Existing balanced front-end patterns in `MicPreamp` establish difference-first processing.
- **Smallest missing engine behavior:** a balanced analog optical-leveler node with fixed timing, a
  soft-knee gain computer, Peak Reduction, output Gain, Compress/Limit, Power, and gain-reduction/output
  readouts. It must compute its detector from `V+ − V−` and drive both output conductors symmetrically.
- **Existing UI widgets to reuse:** `Chassis`, `Row`, `Col`, `Place`, `Control`, `Legend`, `Socket`, and
  the device-handle binding seam.
- **New reusable widget, if justified:** a stepped three-position rotary selector and a compressor VU
  presentation capable of selecting gain-reduction or calibrated output scales. Prefer extending the
  pure presentation seam around `VuFace` rather than forcing the generic `Reading` widget to misstate
  the meter's meaning.
- **Catalog ports/params/configs/readouts:** anticipated runtime params are Peak Reduction, Gain,
  Compress/Limit, Power, and Meter Select. Expose one balanced analog line input, one balanced analog
  line output, gain reduction in dB, and output level in dBu. Exact IDs and whether meter selection
  belongs in the engine param list must be settled in program design against the landed code.
- **Likely files and guardrail tests:** `crates/engine/src/node/`, `crates/engine/src/node.rs`,
  `crates/engine/src/lib.rs`, `crates/devices/src/catalog.rs`, `web/src/widgets/LumaLeveler.svelte`,
  `web/src/device-ui.ts`, relevant reusable meter/selector widget files, engine numeric tests, catalog
  tests, and `web/test/faceplate.test.ts`.
- **Scene schema impact and reason:** none expected. Adding a device type without changing persisted
  scene semantics or existing positional IDs does not require a schema bump.

## Workbench acceptance

- **Browser path used:** not yet exercised; use `/devices/luma_leveler` after implementation.
- **WASM and AudioWorklet smoke-test result:** not yet run.
- **Browser viewport, browser zoom, and workbench zoom:** to be fixed and recorded during the visual loop.
- **Reference front view and fixed zoom:** current silver-face hardware reissue; comparison not yet run.
- **Reference rear view and fixed zoom:** current reissue/manual chassis view; comparison not yet run.
- **Proportion/grouping checks:** pending implementation.
- **Interaction and meter-motion checks:** pending implementation.
- **Supporting devices and representative patch:** patch a suitable analog source or DA output into the
  Luma Leveler, then route its output to the bench monitor/speaker path. Exact supporting cast depends
  on what the landed workbench makes available.
- **Console/network issues or unverified capabilities:** all browser, WASM, AudioWorklet, interaction,
  audibility, and visual claims remain unverified.
- **User visual approval:** scope approved; rendered faceplate approval still required before detailed DSP.

## Later sonic increments

1. **Program-dependent optical release.** A long or heavily reduced signal should recover more slowly
   than a short transient. Educational purpose: demonstrate optical memory. Oracle: two equal-depth test
   signals with different dwell times produce measurably different late-release trajectories.
2. **Frequency-dependent sidechain emphasis.** Turning the maintenance-style emphasis control should
   make high-frequency energy provoke more reduction relative to low-frequency energy. Educational
   purpose: connect detector filtering to compression behavior. Oracle: equal-level low/high sine bursts
   produce the hand-calculated ordering of gain reduction at two control settings.
3. **Modest tube/transformer identity.** Higher output drive should introduce bounded low-order harmonic
   content and bandwidth character without changing domains or hiding conversion. Educational purpose:
   distinguish gain-control behavior from amplifier coloration. Oracle: level sweep measures the intended
   harmonic trend, headroom, and frequency-response tolerances deterministically.
