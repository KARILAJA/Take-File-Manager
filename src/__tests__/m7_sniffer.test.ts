import { describe, it, expect, beforeEach, vi } from "vitest";
import fs from "node:fs";
import path from "node:path";
import { prepareVideoFormatOptions, prepareAudioFormatOptions } from "../utils/mediaFormats";

const bgCode = fs.readFileSync(path.resolve(__dirname, "../../extension/background.js"), "utf8");
const mod: { exports: Record<string, any> } = { exports: {} };
new Function("module", "exports", bgCode)(mod, mod.exports);
const ext = mod.exports;

describe("M7 Browser Extension Video Sniffer (webRequest.onHeadersReceived)", () => {
  let setBadgeMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    ext.detectedMediaByTab.clear();
    setBadgeMock = vi.fn();

    (globalThis as unknown as { chrome: unknown }).chrome = {
      action: {
        setBadgeText: setBadgeMock,
      },
    };
  });

  it("detects HLS, DASH, and video/audio responses per tab and ignores HTML", () => {
    // Non-media should be ignored
    const htmlRes = ext.detectMediaResponse({
      tabId: 10,
      url: "https://video.example.com/watch",
      responseHeaders: [{ name: "Content-Type", value: "text/html; charset=utf-8" }],
    });
    expect(htmlRes).toBeNull();

    // HLS playlist detected
    const hlsRes = ext.detectMediaResponse({
      tabId: 10,
      url: "https://cdn.example.com/stream/master.m3u8",
      responseHeaders: [{ name: "Content-Type", value: "application/vnd.apple.mpegurl" }],
    });
    expect(hlsRes).not.toBeNull();
    expect(hlsRes.kind).toBe("HLS");

    // DASH manifest detected
    const dashRes = ext.detectMediaResponse({
      tabId: 10,
      url: "https://cdn.example.com/stream/manifest.mpd",
      responseHeaders: [{ name: "Content-Type", value: "application/dash+xml" }],
    });
    expect(dashRes).not.toBeNull();
    expect(dashRes.kind).toBe("DASH");

    // Direct MP4 detected with Content-Length
    const mp4Res = ext.detectMediaResponse({
      tabId: 10,
      url: "https://cdn.example.com/clip.mp4",
      responseHeaders: [
        { name: "Content-Type", value: "video/mp4" },
        { name: "Content-Length", value: "12582912" },
      ],
    });
    expect(mp4Res.kind).toBe("Video");
    expect(mp4Res.size).toBe(12582912);

    // Duplicate URL should not double-count
    const dupRes = ext.detectMediaResponse({
      tabId: 10,
      url: "https://cdn.example.com/clip.mp4",
      responseHeaders: [{ name: "Content-Type", value: "video/mp4" }],
    });
    expect(dupRes).toBeNull();

    expect(ext.detectedMediaByTab.get(10)).toHaveLength(3);
    expect(setBadgeMock).toHaveBeenLastCalledWith({ text: "3", tabId: 10 });
  });

  it("ignores noisy googlevideo range chunks and detects video page URLs", () => {
    const chunkRes = ext.detectMediaResponse({
      tabId: 11,
      url: "https://rr3---sn-abc.googlevideo.com/videoplayback?expire=123&mime=video%2Fmp4&range=0-99999",
      responseHeaders: [
        { name: "Content-Type", value: "video/mp4" },
        { name: "Content-Length", value: "100000" },
      ],
    });
    expect(chunkRes).toBeNull();
    expect(ext.isSupportedVideoPageUrl("https://www.youtube.com/watch?v=dQw4w9WgXcQ")).toBe(true);
    expect(ext.isSupportedVideoPageUrl("https://www.tiktok.com/@creator/video/123456")).toBe(true);
    expect(ext.isSupportedVideoPageUrl("https://example.com/article")).toBe(false);
  });

  it("ranks HTTPS formats with exact file sizes ahead of m3u8 duplicates and shows all resolutions", () => {
    const sampleFormats = [
      { format_id: "232", ext: "mp4", resolution: "1280x720", height: 720, vcodec: "avc1.4D401F", acodec: "none", protocol: "m3u8", tbr: 2300, has_video: true, has_audio: false },
      { format_id: "136", ext: "mp4", resolution: "1280x720", height: 720, vcodec: "avc1.4D401F", acodec: "none", protocol: "https", filesize: 55232746, is_filesize_estimated: false, tbr: 909, has_video: true, has_audio: false },
      { format_id: "135", ext: "mp4", resolution: "854x480", height: 480, vcodec: "avc1.4D401F", acodec: "none", protocol: "https", filesize: 23016630, is_filesize_estimated: false, has_video: true, has_audio: false },
      { format_id: "133", ext: "mp4", resolution: "426x240", height: 240, vcodec: "avc1.4D401F", acodec: "none", protocol: "https", filesize: 7143251, is_filesize_estimated: false, has_video: true, has_audio: false },
      { format_id: "160", ext: "mp4", resolution: "256x144", height: 144, vcodec: "avc1.4D400C", acodec: "none", protocol: "https", filesize: 3637102, is_filesize_estimated: false, has_video: true, has_audio: false },
      { format_id: "140", ext: "m4a", vcodec: "none", acodec: "mp4a.40.2", protocol: "https", filesize: 7863053, tbr: 129, has_video: false, has_audio: true },
    ];
    const videos = prepareVideoFormatOptions(sampleFormats);
    expect(videos.map((v) => v.height)).toEqual([720, 480, 240, 144]);
    expect(videos[0].formatId).toBe("136");
    expect(videos[0].sizeSummary).toContain("52.67 MB");
    expect(videos[0].sizeSummary).toContain("7.50 MB");
    expect(videos[0].sizeSummary).toContain("60.17 MB");
    const audios = prepareAudioFormatOptions(sampleFormats);
    expect(audios).toHaveLength(1);
    expect(audios[0].sizeSummary).toBe("7.50 MB");
  });
});
