import { describe, expect, it } from "vitest";
import * as sdk from "../src/index.js";

describe("SDK package exports", () => {
  it("does not expose legacy sandbox websocket entrypoints", () => {
    expect("SandboxWebSocketClient" in sdk).toBe(false);
    expect("WebSocketState" in sdk).toBe(false);
  });
});
