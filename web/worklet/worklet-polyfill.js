// Prepended ahead of the wasm-bindgen glue by build-wasm.sh.
//
// `AudioWorkletGlobalScope` does not expose `TextDecoder`/`TextEncoder` in some browsers (notably
// Chrome — a long-standing gap), yet wasm-bindgen's glue constructs a `TextDecoder` eagerly at load
// time. Without this, that throws a ReferenceError and the whole module fails to evaluate, so
// `registerProcessor` never runs ("node name not defined"). Provide minimal UTF-8 implementations
// when absent. They are only reached when a string crosses the wasm boundary — for the `SceneEngine`
// surface that is panic/error text and patch-build messages, never the audio hot path — so
// correctness matters only for legibility of an error, not for performance.

if (typeof TextDecoder === "undefined") {
  globalThis.TextDecoder = class {
    decode(bytes) {
      if (!bytes) return "";
      const b = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
      let out = "";
      let i = 0;
      while (i < b.length) {
        const c = b[i++];
        if (c < 0x80) {
          out += String.fromCharCode(c);
        } else if (c < 0xe0) {
          out += String.fromCharCode(((c & 0x1f) << 6) | (b[i++] & 0x3f));
        } else if (c < 0xf0) {
          out += String.fromCharCode(((c & 0x0f) << 12) | ((b[i++] & 0x3f) << 6) | (b[i++] & 0x3f));
        } else {
          const cp =
            ((c & 0x07) << 18) | ((b[i++] & 0x3f) << 12) | ((b[i++] & 0x3f) << 6) | (b[i++] & 0x3f);
          const u = cp - 0x10000;
          out += String.fromCharCode(0xd800 + (u >> 10), 0xdc00 + (u & 0x3ff));
        }
      }
      return out;
    }
  };
}

if (typeof TextEncoder === "undefined") {
  globalThis.TextEncoder = class {
    encode(str) {
      const bytes = [];
      for (let i = 0; i < str.length; i++) {
        let cp = str.charCodeAt(i);
        if (cp >= 0xd800 && cp <= 0xdbff && i + 1 < str.length) {
          const lo = str.charCodeAt(i + 1);
          if (lo >= 0xdc00 && lo <= 0xdfff) {
            cp = 0x10000 + ((cp - 0xd800) << 10) + (lo - 0xdc00);
            i++;
          }
        }
        if (cp < 0x80) {
          bytes.push(cp);
        } else if (cp < 0x800) {
          bytes.push(0xc0 | (cp >> 6), 0x80 | (cp & 0x3f));
        } else if (cp < 0x10000) {
          bytes.push(0xe0 | (cp >> 12), 0x80 | ((cp >> 6) & 0x3f), 0x80 | (cp & 0x3f));
        } else {
          bytes.push(
            0xf0 | (cp >> 18),
            0x80 | ((cp >> 12) & 0x3f),
            0x80 | ((cp >> 6) & 0x3f),
            0x80 | (cp & 0x3f),
          );
        }
      }
      return new Uint8Array(bytes);
    }
  };
}
