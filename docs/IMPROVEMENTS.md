# The file contains improvements and bugs that we can tackle between stories

- ~The rack faces are not logical. You can swap an individual piece easily but rack doesn't have any flip button. If the devices are tied to the rack, you should access the back panel of the device either by turning the whole rack around (seeing rack's backside which shows all devicse back sides) or take a device off from the rack and then turn it~ DONE
- ~The analog cable types are not enforced: You can insert XLR cable to TRS socket. These should be handled at engine level~ DONE
- ~Patching is a bit cumbersome. We should stop cable patching if we click while dragging the cable and it's not a valid slot~ DONE
- ~Start the simulation on the page load, no need to click "Start"~ SKIP
- ~Remove the virtual keyboard. Instead, let's add that functionality to a device~ DONE
  - We can have a MIDI keyboard that sends midi events or if it's synth with a keyboard, it accepts the events
  - Devices can have "focus" mode, where you click them (not all, but synths, consoles etc that require more control) and dependning on the device it does different stuff. Keyboards could go to this virtual keyboard mode
  - This is probably a stand alone story
  - DELIVERED as **Story 4.8 — Device focus mode + the interaction seam** (see `IMPLEMENTATION_PLAN.md`): the global QWERTY keyboard is gone; click a synth/console to open a focus surface (an on-screen keybed for instruments, a channel-strip console for `channel_strip`), with note/param input scoped to the focused device. A standalone `midi_controller` device (an `EventThru` node) plays a synth over a MIDI cable — the default scene ships one pre-patched.
- ~Create a proper drawer for catalog, and don't show them inline~ DONE
  - Click catalog button -> Drawer opens where you click the device
- ~Hide the volume and "save load reload" under a menu.~ DONE
- ~Hide global VU meter and simulation info behind debug menu~ DONE
- Do not allow creating new spaces, but describe them in the scene/space file. So from user perspective the layout is hard coded
- Scarlett 8i6 fidelity — full analog/digital I/O (rear line ins, more line outs, S/PDIF), a **routing matrix** in the focus view, **device-level power**, **multichannel digital ports**, **preamp physics** (INST/PAD/AIR/48V), and a **device-dimensions** pass. → **Promoted to `IMPLEMENTATION_PLAN.md` Story 5.7 part 2 (tasks 5.7.5–5.7.10)** — see there for the full rationale + done-states.
