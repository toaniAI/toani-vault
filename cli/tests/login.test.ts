import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CliConfig } from "../src/types/cli.js";

const promptState = vi.hoisted(() => {
  const selectQueue: unknown[] = [];
  const confirmQueue: unknown[] = [];
  const passwordQueue: unknown[] = [];
  const log = {
    step: vi.fn(),
    info: vi.fn(),
    success: vi.fn(),
    warn: vi.fn(),
  };
  const spinnerApi = {
    start: vi.fn(),
    stop: vi.fn(),
    message: vi.fn(),
  };

  return {
    selectQueue,
    confirmQueue,
    passwordQueue,
    intro: vi.fn(),
    outro: vi.fn(),
    note: vi.fn(),
    log,
    spinner: vi.fn(() => spinnerApi),
    isCancel: vi.fn(() => false),
    reset() {
      selectQueue.length = 0;
      confirmQueue.length = 0;
      passwordQueue.length = 0;
      log.step.mockReset();
      log.info.mockReset();
      log.success.mockReset();
      log.warn.mockReset();
      spinnerApi.start.mockReset();
      spinnerApi.stop.mockReset();
      spinnerApi.message.mockReset();
      this.intro.mockReset();
      this.outro.mockReset();
      this.note.mockReset();
      this.spinner.mockClear();
      this.isCancel.mockReset();
      this.isCancel.mockReturnValue(false);
    },
    select: vi.fn(async () => selectQueue.shift()),
    confirm: vi.fn(async () => confirmQueue.shift()),
    password: vi.fn(async () => passwordQueue.shift()),
  };
});

const clipboardMock = vi.hoisted(() => ({
  read: vi.fn(),
}));
const openMock = vi.hoisted(() => vi.fn());
const keychainMock = vi.hoisted(() => ({
  set: vi.fn(),
}));
const validateTokenMock = vi.hoisted(() => vi.fn());
const envMock = vi.hoisted(() => ({
  readTokenFromEnv: vi.fn(),
}));

vi.mock("@clack/prompts", () => ({
  intro: promptState.intro,
  outro: promptState.outro,
  note: promptState.note,
  log: promptState.log,
  spinner: promptState.spinner,
  isCancel: promptState.isCancel,
  select: promptState.select,
  confirm: promptState.confirm,
  password: promptState.password,
}));

vi.mock("clipboardy", () => ({
  default: clipboardMock,
}));

vi.mock("open", () => ({
  default: openMock,
}));

vi.mock("../src/lib/keychain.js", () => ({
  keychain: keychainMock,
}));

vi.mock("../src/lib/env.js", () => envMock);

vi.mock("../src/lib/validate.js", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("../src/lib/validate.js")>();
  return {
    ...actual,
    validateToken: validateTokenMock,
  };
});

import { __testables, runLogin } from "../src/commands/login.js";

const baseConfig: CliConfig = {
  baseUrl: "https://api.example.com",
  output: "table",
  timeout: 30000,
  currentProfile: "default",
  profiles: { default: {} },
  credentialSource: "none",
};

describe("runLogin", () => {
  beforeEach(() => {
    promptState.reset();
    clipboardMock.read.mockReset();
    openMock.mockReset();
    keychainMock.set.mockReset();
    validateTokenMock.mockReset();
    envMock.readTokenFromEnv.mockReset();
    vi.spyOn(console, "log").mockImplementation(() => {});
    vi.spyOn(process.stdout, "write").mockImplementation(() => true);
    vi.spyOn(global, "setInterval").mockImplementation((fn: TimerHandler) => {
      if (typeof fn === "function") {
        fn();
      }
      return 1 as unknown as NodeJS.Timeout;
    });
    vi.spyOn(global, "clearInterval").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("uses env token and skips validation when requested", async () => {
    promptState.selectQueue.push("paste");
    envMock.readTokenFromEnv.mockReturnValue({
      token: `v4.local.${"a".repeat(120)}`,
      source: "process.env",
    });

    await runLogin(baseConfig, ["--skip-validate"]);

    expect(validateTokenMock).not.toHaveBeenCalled();
    expect(keychainMock.set).toHaveBeenCalled();
  });

  it("uses clipboard token when available and validates it", async () => {
    promptState.selectQueue.push("paste");
    envMock.readTokenFromEnv.mockReturnValue(null);
    clipboardMock.read.mockResolvedValue(`v4.local.${"b".repeat(120)}`);
    validateTokenMock.mockResolvedValue({ ok: true, status: 200 });

    await runLogin(baseConfig, []);

    expect(validateTokenMock).toHaveBeenCalledWith(
      "https://api.example.com",
      expect.stringContaining("v4.local."),
    );
    expect(keychainMock.set).toHaveBeenCalled();
  });

  it("uses the shared 3-option menu for the already-have-token path", async () => {
    promptState.selectQueue.push("paste", "paste");
    promptState.passwordQueue.push(`v4.local.${"c".repeat(120)}`);
    envMock.readTokenFromEnv.mockReturnValue(null);
    clipboardMock.read.mockResolvedValue("");
    validateTokenMock.mockResolvedValue({ ok: true, status: 200 });

    await runLogin(baseConfig, []);

    expect(promptState.select.mock.calls[1]?.[0]).toMatchObject({
      message: "How do you want to provide the token?",
      options: [
        expect.objectContaining({ value: "paste" }),
        expect.objectContaining({ value: "env" }),
        expect.objectContaining({ value: "cancel" }),
      ],
    });
    expect(promptState.select.mock.calls[1]?.[0]?.options).toHaveLength(3);
    expect(keychainMock.set).toHaveBeenCalled();
  });

  it("shows the guided flow token menu before clipboard watching", async () => {
    promptState.selectQueue.push("yes", "yes", "paste", "paste");
    promptState.passwordQueue.push(`v4.local.${"d".repeat(120)}`);
    envMock.readTokenFromEnv.mockReturnValue(null);
    validateTokenMock.mockResolvedValue({ ok: true, status: 200 });

    await runLogin(baseConfig, []);

    expect(promptState.select.mock.calls[2]?.[0]).toMatchObject({
      message: "How do you want to provide the token?",
      initialValue: "auto",
      options: [
        expect.objectContaining({ value: "auto" }),
        expect.objectContaining({ value: "paste" }),
        expect.objectContaining({ value: "env" }),
        expect.objectContaining({ value: "cancel" }),
      ],
    });
    expect(promptState.select.mock.calls[2]?.[0]?.options).toHaveLength(4);
    expect(promptState.select.mock.calls[3]?.[0]?.options).toHaveLength(3);
  });

  it("rechecks .env through the shared token entry flow", async () => {
    promptState.selectQueue.push("env");
    promptState.confirmQueue.push(true);
    envMock.readTokenFromEnv
      .mockReturnValueOnce(null)
      .mockReturnValueOnce({
        token: `v4.local.${"e".repeat(120)}`,
        source: "/tmp/.env",
      });
    validateTokenMock.mockResolvedValue({ ok: true, status: 200 });

    await __testables.requestTokenInput("https://api.example.com", false);

    expect(promptState.note).toHaveBeenCalledWith(
      expect.stringContaining('TOANI_VAULT_TOKEN="v4.local.your-token-here..."'),
      expect.any(String),
      expect.any(Object),
    );
    expect(promptState.confirm).toHaveBeenCalled();
    expect(keychainMock.set).toHaveBeenCalled();
  });

  it("maps validation failures to a detailed note without saving", async () => {
    validateTokenMock.mockResolvedValue({
      ok: false,
      status: 401,
      reason: "invalid_or_expired",
    });

    await __testables.processToken(`v4.local.${"c".repeat(120)}`, "https://api.example.com", false);

    expect(promptState.note).toHaveBeenCalledWith(
      expect.stringContaining("HTTP 401"),
      expect.stringContaining("Token invalid"),
    );
    expect(keychainMock.set).not.toHaveBeenCalled();
  });

  it("warns and continues when keychain persistence fails", async () => {
    validateTokenMock.mockResolvedValue({ ok: true, status: 200 });
    keychainMock.set.mockImplementation(() => {
      throw new Error("keychain unavailable");
    });

    await __testables.processToken(`v4.local.${"d".repeat(120)}`, "https://api.example.com", false);

    expect(promptState.log.warn).toHaveBeenCalledWith(
      expect.stringContaining("Token was not persisted"),
    );
    expect(promptState.outro).toHaveBeenCalledWith(
      expect.stringContaining("🎉 Connected!"),
    );
  });
});
