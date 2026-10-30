import { describe, expect, it } from "vitest";
import {
  createSocketGeometryIndex,
  type FaceGeometry,
  geometryRevision,
  projectSocket,
  registerGeometry,
  requestGeometry,
  socketFor,
  unregisterGeometry,
} from "../src/socket-geometry";

const FRONT = { face: "studio:mixer", side: "front" } as const;
const BACK = { face: "studio:mixer", side: "back" } as const;

const geometry = (sockets: FaceGeometry["sockets"] = []): FaceGeometry => ({
  width: 200,
  height: 50,
  sockets,
});
const jack = (id: string, x: number, aliases: string[] = [id]) => ({ id, aliases, x, y: 20, r: 3 });

describe("socket geometry index", () => {
  it("is pending until the requested generation registers, then current", () => {
    const req = requestGeometry(createSocketGeometryIndex(), FRONT, "r1");
    expect(req.measure).toBe(true);
    expect(socketFor(req.index, FRONT, "mixer:output:0")).toEqual({ status: "pending" });

    const reg = registerGeometry(
      req.index,
      FRONT,
      req.generation,
      geometry([jack("mixer:output:0", 40)]),
    );
    expect(reg.outcome).toBe("current");
    expect(socketFor(reg.index, FRONT, "mixer:output:0")).toMatchObject({
      status: "current",
      socket: { x: 40 },
    });
    expect(socketFor(reg.index, BACK, "mixer:output:0")).toEqual({ status: "absent" });
  });

  it("keeps current geometry for an unchanged revision (camera/placement are not inputs)", () => {
    const req = requestGeometry(createSocketGeometryIndex(), FRONT, geometryRevision(["a", 1]));
    const reg = registerGeometry(req.index, FRONT, req.generation, geometry([jack("j", 10)]));
    const again = requestGeometry(reg.index, FRONT, geometryRevision(["a", 1]));
    expect(again.measure).toBe(false);
    expect(again.index).toBe(reg.index);
  });

  it("invalidates on a revision change and rejects the older generation's late measurement", () => {
    const first = requestGeometry(createSocketGeometryIndex(), FRONT, "r1");
    const second = requestGeometry(first.index, FRONT, "r2");
    expect(second.generation).toBeGreaterThan(first.generation);

    const late = registerGeometry(second.index, FRONT, first.generation, geometry([jack("j", 1)]));
    expect(late.outcome).toBe("stale");
    expect(socketFor(late.index, FRONT, "j")).toEqual({ status: "pending" });
  });

  it("rejects empty layout boxes and duplicate sockets, leaving the slot pending", () => {
    const req = requestGeometry(createSocketGeometryIndex(), FRONT, "r1");
    const empty = registerGeometry(req.index, FRONT, req.generation, {
      width: 0,
      height: 0,
      sockets: [],
    });
    expect(empty.outcome).toBe("empty");

    const dup = registerGeometry(
      req.index,
      FRONT,
      req.generation,
      geometry([jack("j", 10), jack("j", 30)]),
    );
    expect(dup.outcome).toBe("duplicate");
    expect(socketFor(dup.index, FRONT, "j")).toEqual({ status: "pending" });
  });

  it("registers a duplex connector once and resolves both endpoint aliases to it", () => {
    const req = requestGeometry(createSocketGeometryIndex(), BACK, "r1");
    const duplex = jack("pc:output:1", 120, ["pc:output:1", "pc:input:1"]);
    const reg = registerGeometry(req.index, BACK, req.generation, geometry([duplex]));
    expect(reg.outcome).toBe("current");
    const out = socketFor(reg.index, BACK, "pc:output:1");
    const inn = socketFor(reg.index, BACK, "pc:input:1");
    expect(out.status === "current" && inn.status === "current").toBe(true);
    if (out.status === "current" && inn.status === "current") {
      expect(out.socket).toBe(inn.socket);
    }
  });

  it("answers a measurement for an unregistered slot as unmounted, and never reuses its token", () => {
    const req = requestGeometry(createSocketGeometryIndex(), FRONT, "r1");
    const gone = unregisterGeometry(req.index, FRONT);
    expect(registerGeometry(gone, FRONT, req.generation, geometry()).outcome).toBe("unmounted");

    const remount = requestGeometry(gone, FRONT, "r1");
    expect(remount.generation).not.toBe(req.generation);
    expect(registerGeometry(remount.index, FRONT, req.generation, geometry()).outcome).toBe(
      "stale",
    );
  });

  it("keys differently configured instances of one type independently", () => {
    const a = { face: "studio:pc-a", side: "back" } as const;
    const b = { face: "studio:pc-b", side: "back" } as const;
    let index = createSocketGeometryIndex();
    const ra = requestGeometry(index, a, geometryRevision(["computer", "usb=2"]));
    index = ra.index;
    const rb = requestGeometry(index, b, geometryRevision(["computer", "usb=8"]));
    index = registerGeometry(
      rb.index,
      a,
      ra.generation,
      geometry([jack("pc-a:output:0", 10)]),
    ).index;
    index = registerGeometry(index, b, rb.generation, geometry([jack("pc-b:output:0", 90)])).index;
    expect(socketFor(index, a, "pc-a:output:0")).toMatchObject({ socket: { x: 10 } });
    expect(socketFor(index, b, "pc-b:output:0")).toMatchObject({ socket: { x: 90 } });
  });
});

describe("socket projection", () => {
  const g = geometry([jack("j", 40)]);
  const [socket] = g.sockets;

  it("scales face-local mm into the frame for a presented side", () => {
    // frame 400 wide for a 200 mm face ⇒ 2 units/mm: x = 100 + 40·2 = 180, y = 10 + 20·2 = 50
    expect(projectSocket(g, socket, { x: 100, y: 10, width: 400, height: 100 }, true)).toEqual({
      x: 180,
      y: 50,
    });
  });

  it("mirrors a see-through side about the face width", () => {
    // hidden side: x = 100 + (200 − 40)·2 = 420
    expect(projectSocket(g, socket, { x: 100, y: 10, width: 400, height: 100 }, false)).toEqual({
      x: 420,
      y: 50,
    });
  });

  it("depends only on the frame, so camera and placement moves need no remeasurement", () => {
    const at = (x: number) => projectSocket(g, socket, { x, y: 0, width: 200, height: 50 }, true);
    expect(at(1000).x - at(0).x).toBe(1000);
  });
});
