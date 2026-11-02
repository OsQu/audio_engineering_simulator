# AES-70 worklog

## 2026-11-02 — planning
- Gap pass against 10.6 tip `0003465`: DOM-exact `[data-jack]` targets; four competing pointerdown starts
  (draggable, camera backdrop, window patch handler, cable onclick); drag release commits the last move's
  cached verdict; no pointer id/capture/cancel on patch; verdict reasons never rendered; focus windows
  already `data-interaction="disabled"`.
- Wrote companion design + thin Story block + this plan. Next: Oskari's design review before any code.

## 2026-11-02 — parked
- Asked Oskari to review the Story 10.7 design (I-222): decisions 1–7, especially #5 (illegal drag release
  → pending pick with visible reason). No answer within 15 min → parked. Waiting for design sign-off before
  implementing Task 10.7.1. No code written yet.
