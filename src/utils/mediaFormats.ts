import { MediaFormat } from "../store";
import { formatBytes } from "./format";

export interface PreparedMediaOption {
  formatId: string;
  submitValue: string;
  audioOnly: boolean;
  resolutionLabel: string;
  extLabel: string;
  codecLabel: string;
  fpsLabel: string;
  needsAudioMerge: boolean;
  baseSizeBytes?: number;
  audioSizeBytes?: number;
  totalSizeBytes?: number;
  isApproxSize: boolean;
  sizeSummary: string;
  bitrateLabel: string;
  protocolLabel: string;
  height: number;
}

export function shortCodecName(codec?: string): string {
  if (!codec || codec === "none") return "";
  const low = codec.toLowerCase();
  if (low.startsWith("avc1") || low.includes("h264")) return "H.264";
  if (low.startsWith("vp09") || low.startsWith("vp9")) return "VP9";
  if (low.startsWith("av01")) return "AV1";
  if (low.startsWith("hev1") || low.startsWith("hvc1") || low.includes("hevc")) return "HEVC";
  if (low.startsWith("mp4a")) return "AAC";
  if (low.startsWith("opus")) return "Opus";
  return codec.split(".")[0].toUpperCase();
}

function protocolRank(protocol?: string): number {
  const p = (protocol || "").toLowerCase();
  if (p === "https" || p === "http") return 3;
  if (p.includes("dash")) return 2;
  if (p.includes("m3u8")) return 0;
  return 1;
}

function codecRank(vcodec?: string): number {
  const c = shortCodecName(vcodec);
  if (c === "H.264") return 3;
  if (c === "VP9") return 2;
  if (c === "AV1") return 1;
  return 0;
}

export function prepareVideoFormatOptions(formats: MediaFormat[]): PreparedMediaOption[] {
  const valid = (formats || []).filter(
    (f) => f.ext?.toLowerCase() !== "mhtml" && (f.has_video || f.has_audio)
  );

  // Best audio stream sizes per container for accurate "+Audio" total estimates
  const audioStreams = valid
    .filter((f) => !f.has_video && f.has_audio && f.filesize && f.filesize > 0)
    .sort((a, b) => (b.filesize || 0) - (a.filesize || 0));

  const bestM4aAudio = audioStreams.find((a) => a.ext === "m4a")?.filesize;
  const bestWebmAudio = audioStreams.find((a) => a.ext === "webm")?.filesize;
  const bestAnyAudio = audioStreams[0]?.filesize;

  // Sort videos: highest resolution first -> exact HTTPS streams before m3u8 -> exact size -> H.264 > VP9 > AV1
  const videos = valid
    .filter((f) => f.has_video)
    .sort((a, b) => {
      const hDiff = (b.height || 0) - (a.height || 0);
      if (hDiff !== 0) return hDiff;
      const pDiff = protocolRank(b.protocol) - protocolRank(a.protocol);
      if (pDiff !== 0) return pDiff;
      const eDiff = Number(!b.is_filesize_estimated) - Number(!a.is_filesize_estimated);
      if (eDiff !== 0) return eDiff;
      const sDiff = Number(Boolean(b.filesize)) - Number(Boolean(a.filesize));
      if (sDiff !== 0) return sDiff;
      return codecRank(b.vcodec) - codecRank(a.vcodec);
    });

  // Keep one clean representative per (height, ext, codec, has_audio) so ALL resolutions & codecs appear without m3u8 duplicates
  const seenKeys = new Set<string>();
  const out: PreparedMediaOption[] = [];

  for (const f of videos) {
    const height = f.height || 0;
    const resLabel = height > 0 ? `${height}p` : f.resolution || f.ext.toUpperCase();
    const codec = shortCodecName(f.vcodec);
    const extUpper = (f.ext || "mp4").toUpperCase();
    const dedupKey = `${resLabel}|${extUpper}|${codec}|${f.has_audio ? "mux" : "sep"}`;
    if (seenKeys.has(dedupKey)) continue;
    seenKeys.add(dedupKey);

    const needsMerge = !f.has_audio && f.format_id !== "direct";
    const matchedAudio = needsMerge
      ? extUpper === "WEBM"
        ? bestWebmAudio || bestAnyAudio
        : bestM4aAudio || bestAnyAudio
      : undefined;

    const baseSize = f.filesize && f.filesize > 0 ? f.filesize : undefined;
    const totalSize =
      baseSize !== undefined
        ? baseSize + (needsMerge && matchedAudio ? matchedAudio : 0)
        : undefined;
    const isApprox = Boolean(f.is_filesize_estimated) || Boolean(needsMerge && matchedAudio);

    let sizeSummary = "--";
    if (totalSize !== undefined) {
      if (needsMerge && matchedAudio && baseSize !== undefined) {
        sizeSummary = `${f.is_filesize_estimated ? "~" : ""}${formatBytes(baseSize)} + ${formatBytes(matchedAudio)} ≈ ${formatBytes(totalSize)}`;
      } else {
        sizeSummary = `${isApprox ? "~" : ""}${formatBytes(totalSize)}`;
      }
    }

    const submitValue =
      f.has_audio || f.format_id === "direct"
        ? f.format_id
        : `${f.format_id}+ba/${f.format_id}`;

    out.push({
      formatId: f.format_id,
      submitValue,
      audioOnly: false,
      resolutionLabel: f.resolution && height > 0 ? `${resLabel} (${f.resolution})` : resLabel,
      extLabel: extUpper,
      codecLabel: codec || "--",
      fpsLabel: f.fps ? `${Math.round(f.fps)} fps` : "--",
      needsAudioMerge: needsMerge,
      baseSizeBytes: baseSize,
      audioSizeBytes: matchedAudio,
      totalSizeBytes: totalSize,
      isApproxSize: isApprox,
      sizeSummary,
      bitrateLabel: f.tbr ? `${Math.round(f.tbr)} kbps` : "--",
      protocolLabel: (f.protocol || "https").toUpperCase(),
      height,
    });
  }

  return out;
}

export function prepareAudioFormatOptions(formats: MediaFormat[]): PreparedMediaOption[] {
  const audios = (formats || [])
    .filter((f) => !f.has_video && f.has_audio && f.ext?.toLowerCase() !== "mhtml")
    .sort((a, b) => (b.tbr || 0) - (a.tbr || 0));

  const seenKeys = new Set<string>();
  const out: PreparedMediaOption[] = [];

  for (const f of audios) {
    const extUpper = (f.ext || "m4a").toUpperCase();
    const codec = shortCodecName(f.acodec);
    const kbps = f.tbr ? Math.round(f.tbr) : 0;
    const key = `${extUpper}|${codec}|${kbps}`;
    if (seenKeys.has(key)) continue;
    seenKeys.add(key);

    const sizeSummary =
      f.filesize && f.filesize > 0
        ? `${f.is_filesize_estimated ? "~" : ""}${formatBytes(f.filesize)}`
        : "--";

    out.push({
      formatId: f.format_id,
      submitValue: f.format_id,
      audioOnly: false, // direct audio stream id download
      resolutionLabel: f.format_note ? `Audio (${f.format_note})` : "Audio Saja",
      extLabel: extUpper,
      codecLabel: codec || "--",
      fpsLabel: "--",
      needsAudioMerge: false,
      baseSizeBytes: f.filesize,
      totalSizeBytes: f.filesize,
      isApproxSize: Boolean(f.is_filesize_estimated),
      sizeSummary,
      bitrateLabel: kbps > 0 ? `${kbps} kbps` : "--",
      protocolLabel: (f.protocol || "https").toUpperCase(),
      height: 0,
    });
  }

  return out;
}
