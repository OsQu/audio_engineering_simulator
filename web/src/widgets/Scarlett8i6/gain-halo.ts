// Scarlett 8i6 input-gain LED rings. The engine exposes the input peaks in dBu, while the reference
// device labels its tri-colour thresholds in dBFS. Its modeled ADC is full scale at 1 V peak, which is
// 20·log10(1 / 0.7746) dBu; translate the documented -24 / -6 / 0 dBFS boundaries into that readout
// scale here, rather than teaching the general DeviceHandle about this one chassis calibration.

export type ScarlettGainHaloState = "off" | "green" | "amber" | "red";

const ADC_FULL_SCALE_DBU = 20 * Math.log10(1 / 0.7746);

export const SCARLETT_GAIN_HALO_THRESHOLDS_DBU = {
  green: ADC_FULL_SCALE_DBU - 24,
  amber: ADC_FULL_SCALE_DBU - 6,
  red: ADC_FULL_SCALE_DBU,
} as const;

/** Maps an input peak readout (dBu) onto the Scarlett's documented LED-ring state. */
export function scarlettGainHaloState(peakDbu: number): ScarlettGainHaloState {
  // Silence is represented as -∞ by level readouts. Treat every non-finite value as unlit so a stale
  // or invalid UI value cannot create a false signal/clip indication.
  if (!Number.isFinite(peakDbu) || peakDbu < SCARLETT_GAIN_HALO_THRESHOLDS_DBU.green) return "off";
  if (peakDbu < SCARLETT_GAIN_HALO_THRESHOLDS_DBU.amber) return "green";
  if (peakDbu < SCARLETT_GAIN_HALO_THRESHOLDS_DBU.red) return "amber";
  return "red";
}
