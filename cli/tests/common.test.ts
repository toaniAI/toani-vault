import { describe, expect, it } from "vitest";
import { parseOptions } from "../src/commands/common.js";

describe("parseOptions", () => {
  it("parses key value and positional args", () => {
    const parsed = parseOptions(["create", "--foo", "bar", "--flag"]);
    expect(parsed._).toEqual(["create"]);
    expect(parsed.foo).toBe("bar");
    expect(parsed.flag).toBe(true);
  });
});
