import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const skillText = readFileSync(join(process.cwd(), "SKILL.md"), "utf8");

describe("CLI skill contract", () => {
  it("documents the approval-aware sandbox contract", () => {
    expect(skillText).toContain("Approval-Aware Flow");
    expect(skillText).toContain("toani-vault approvals generate-request-id");
    expect(skillText).toContain("credential_runtime_access");
    expect(skillText).toContain("toani-vault sandbox request");
    expect(skillText).toContain("toani-vault sandbox get-request");
    expect(skillText).toContain("http_request");
  });

  it("does not advertise retired session lifecycle commands", () => {
    expect(skillText).not.toContain("create-session");
    expect(skillText).not.toContain("list-sessions");
    expect(skillText).not.toContain("get-session");
    expect(skillText).not.toContain("terminate");
    expect(skillText).not.toContain("sandbox stats");
  });

  it("does not advertise retired browser operation aliases", () => {
    expect(skillText).not.toContain("bootstrap-page");
    expect(skillText).not.toContain("export-dom");
    expect(skillText).not.toContain("export-data");
  });

  it("keeps approval guidance explicit in execution plans", () => {
    expect(skillText).toContain("must explicitly include");
    expect(skillText).toContain("request id generation");
    expect(skillText).toContain("sandbox execution with the same request id");
  });
});
