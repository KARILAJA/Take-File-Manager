import { describe, it, expect, beforeEach, vi } from "vitest";
import fs from "node:fs";
import path from "node:path";

const bgCode = fs.readFileSync(path.resolve(__dirname, "../../extension/background.js"), "utf8");
const mod: { exports: Record<string, any> } = { exports: {} };
new Function("module", "exports", bgCode)(mod, mod.exports);
const ext = mod.exports;

describe("M6 Browser Extension MV3 (Capture Rules, Cancel & Cookies)", () => {
  let cancelMock: ReturnType<typeof vi.fn>;
  let eraseMock: ReturnType<typeof vi.fn>;
  let sendNativeMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    cancelMock = vi.fn().mockResolvedValue(undefined);
    eraseMock = vi.fn().mockResolvedValue(undefined);
    sendNativeMock = vi.fn((_host, _msg, cb) => {
      cb({ ok: true, id: "dl-ext-99" });
    });

    (globalThis as unknown as { chrome: unknown }).chrome = {
      storage: {
        local: {
          get: vi.fn().mockResolvedValue({
            turbogetSettings: {
              ...ext.DEFAULT_SETTINGS,
              excludedHosts: ["ignored.example.com"],
            },
          }),
        },
      },
      cookies: {
        getAll: vi.fn().mockResolvedValue([
          { name: "session", value: "abc123" },
          { name: "csrf", value: "tok999" },
        ]),
      },
      downloads: {
        cancel: cancelMock,
        erase: eraseMock,
      },
      runtime: {
        lastError: null,
        sendNativeMessage: sendNativeMock,
      },
    };
  });

  it("intercepts matching download, cancels Chrome download, and forwards cookies to native host", async () => {
    const result = await ext.handleDownloadCreated({
      id: 42,
      url: "https://files.example.org/release_v2.zip?token=1",
      finalUrl: "https://cdn.example.org/release_v2.zip?token=1",
      filename: "C:\\Users\\Test\\Downloads\\release_v2.zip",
      referrer: "https://files.example.org/download",
      mime: "application/zip",
      fileSize: 5_000_000,
    });

    expect(result.intercepted).toBe(true);
    expect(result.response).toEqual({ ok: true, id: "dl-ext-99" });
    expect(cancelMock).toHaveBeenCalledWith(42);
    expect(eraseMock).toHaveBeenCalledWith({ id: 42 });

    expect(sendNativeMock).toHaveBeenCalledTimes(1);
    const sentMsg = sendNativeMock.mock.calls[0][1];
    expect(sentMsg.type).toBe("add_download");
    expect(sentMsg.payload.url).toBe("https://cdn.example.org/release_v2.zip?token=1");
    expect(sentMsg.payload.cookies).toBe("session=abc123; csrf=tok999");
    expect(sentMsg.payload.referrer).toBe("https://files.example.org/download");
    expect(sentMsg.payload.fileName).toBe("release_v2.zip");
    expect(sentMsg.payload.mime).toBe("application/zip");
    expect(sentMsg.payload.askSaveLocation).toBe(true);
  });

  it("does not intercept excluded host or small non-matching files", async () => {
    const excludedRes = await ext.handleDownloadCreated({
      id: 43,
      url: "https://sub.ignored.example.com/archive.zip",
      filename: "archive.zip",
      fileSize: 10_000_000,
    });

    expect(excludedRes.intercepted).toBe(false);
    expect(cancelMock).not.toHaveBeenCalled();

    const smallTxtRes = await ext.handleDownloadCreated({
      id: 44,
      url: "https://normal.example.com/notes.txt",
      filename: "notes.txt",
      fileSize: 2048,
    });

    expect(smallTxtRes.intercepted).toBe(false);
    expect(cancelMock).not.toHaveBeenCalled();
  });

  it("deduplicates concurrent onCreated and onDeterminingFilename events for same download", async () => {
    const item = {
      id: 99,
      url: "https://files.example.org/concurrent.zip",
      finalUrl: "https://files.example.org/concurrent.zip",
      filename: "concurrent.zip",
      referrer: "https://files.example.org",
      mime: "application/zip",
      fileSize: 2_000_000,
    };

    const [r1, r2] = await Promise.all([
      ext.handleDownloadCreated(item),
      ext.handleDownloadCreated(item),
    ]);

    expect([r1.intercepted, r2.intercepted].filter(Boolean)).toHaveLength(1);
    expect(sendNativeMock).toHaveBeenCalledTimes(1);
  });
});
