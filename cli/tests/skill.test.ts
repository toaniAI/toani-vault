import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const skillText = readFileSync(join(process.cwd(), "SKILL.md"), "utf8");

describe("CLI skill contract", () => {
  it("documents the Lightpanda-backed sandbox contract", () => {
    expect(skillText).toContain("Lightpanda");
    expect(skillText).toContain("puppeteer-core");
    expect(skillText).toContain("http_request");
    expect(skillText).toContain("export-dom");
    expect(skillText).toContain("bootstrap-page");
    expect(skillText).toContain("Rocket Loader");
  });

  it("does not advertise removed browser operation types as supported", () => {
    const supportedSection = skillText.split("不要使用这些旧操作名")[0];

    expect(supportedSection).not.toContain("- `get_attribute`");
    expect(supportedSection).not.toContain("- `screenshot`");
  });

  it("documents bootstrap as credential-safe and keeps credential use in fill", () => {
    expect(skillText).toContain("bootstrap-page");
    expect(skillText).toContain("不消费凭证");
    expect(skillText).toContain("fill");
  });
});
