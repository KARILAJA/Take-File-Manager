import React, { useEffect, useState } from "react";
import {
  X,
  Search,
  CheckCircle,
  AlertTriangle,
  Download,
  Folder,
  FileUp,
  Clipboard,
  Trash2,
  Film,
} from "lucide-react";
import { useAppStore, ProbeResult, MediaProbeResult } from "../store";
import { formatBytes } from "../utils/format";
import { prepareVideoFormatOptions, prepareAudioFormatOptions } from "../utils/mediaFormats";

function isLikelyVideoGrabUrl(raw: string): boolean {
  const low = raw.trim().toLowerCase();
  if (!low.startsWith("http://") && !low.startsWith("https://")) return false;
  if (low.includes(".m3u8") || low.includes(".mpd")) return true;
  return /youtube\.com\/(watch|shorts|live)|youtu\.be\/|tiktok\.com\/|instagram\.com\/(reel|reels|p\/)|facebook\.com\/(watch|reel|videos)|fb\.watch\/|vimeo\.com\/\d+|dailymotion\.com\/video|twitch\.tv\/(videos|clip)|bilibili\.com\/video|(twitter|x)\.com\/.+\/status\//.test(
    low
  );
}

export const AddUrlModal: React.FC = () => {
  const {
    isAddModalOpen,
    setAddModalOpen,
    pendingPrompt,
    setPendingPrompt,
    queues,
    probeUrl,
    mediaProbe,
    addDownload,
    getDefaultDownloadDir,
    pickFolder,
    pickTorrentFile,
    t,
  } = useAppStore();
  const [url, setUrl] = useState("");
  const [probing, setProbing] = useState(false);
  const [probeResult, setProbeResult] = useState<ProbeResult | null>(null);
  const [mediaProbeResult, setMediaProbeResult] = useState<MediaProbeResult | null>(null);
  const [isMediaMode, setIsMediaMode] = useState(false);
  const [selectedFormatId, setSelectedFormatId] = useState("best");
  const [fileName, setFileName] = useState("");
  const [saveDir, setSaveDir] = useState("");
  const [categoryId, setCategoryId] = useState("");
  const [queueId, setQueueId] = useState("");
  const [segments, setSegments] = useState(8);
  const [headers, setHeaders] = useState<[string, string][]>([]);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    if (!isAddModalOpen) return;

    if (pendingPrompt) {
      setUrl(pendingPrompt.url);
      setFileName(pendingPrompt.file_name || "");
      setSaveDir(pendingPrompt.save_dir || "");
      setSegments(pendingPrompt.segments_cfg || 8);
      setHeaders(pendingPrompt.headers || []);
      setProbeResult(null);
      setMediaProbeResult(null);
      const mediaFlag = Boolean(pendingPrompt.is_media || isLikelyVideoGrabUrl(pendingPrompt.url));
      setIsMediaMode(mediaFlag);
      setSelectedFormatId(
        pendingPrompt.audio_only
          ? "audio_only"
          : pendingPrompt.format_id || "best"
      );
      if (mediaFlag) {
        setCategoryId(pendingPrompt.audio_only ? "music" : "video");
      }

      setProbing(true);
      if (mediaFlag) {
        mediaProbe(pendingPrompt.url, pendingPrompt.headers)
          .then((res) => {
            setMediaProbeResult(res);
            if (!pendingPrompt.file_name && res.title) {
              setFileName(`${res.title}.${pendingPrompt.audio_only ? "mp3" : "mp4"}`);
            }
          })
          .catch(() => {})
          .finally(() => setProbing(false));
      } else {
        probeUrl(pendingPrompt.url)
          .then((res) => {
            setProbeResult(res);
            if (!pendingPrompt.file_name && res.file_name) {
              setFileName(res.file_name);
            }
          })
          .catch(() => {})
          .finally(() => setProbing(false));
      }
    } else if (!saveDir) {
      getDefaultDownloadDir().then((dir) => {
        if (dir) setSaveDir(dir);
      });
    }
  }, [isAddModalOpen, pendingPrompt]);

  if (!isAddModalOpen) return null;

  const detectProtocolBadge = (raw: string) => {
    const low = raw.trim().toLowerCase();
    if (low.startsWith("magnet:")) return "MAGNET:";
    if (low.endsWith(".torrent")) return "TORRENT";
    if (low.startsWith("ftp://") || low.startsWith("ftps://")) return "FTP://";
    if (low.startsWith("http://")) return "HTTP://";
    return "HTTPS://";
  };

  const handleClose = () => {
    setAddModalOpen(false);
    setPendingPrompt(null);
    setHeaders([]);
  };

  const handlePasteClipboard = async () => {
    try {
      if (navigator.clipboard?.readText) {
        const text = await navigator.clipboard.readText();
        if (text) setUrl(text.trim());
      }
    } catch {
      // Clipboard API unavailable in headless env
    }
  };

  const handleBrowseFolder = async () => {
    const chosen = await pickFolder(saveDir || undefined);
    if (chosen) {
      setSaveDir(chosen);
    }
  };

  const handlePickTorrent = async () => {
    const chosen = await pickTorrentFile();
    if (!chosen) return;
    setUrl(chosen);
    setCategoryId("torrent");
    setProbing(true);
    try {
      const res = await probeUrl(chosen);
      setProbeResult(res);
      setFileName(res.file_name);
    } catch (e) {
      console.error(e);
      alert("Gagal membaca file torrent: " + e);
    } finally {
      setProbing(false);
    }
  };

  const handleProbe = async () => {
    if (!url.trim()) return;
    const cleanUrl = url.trim();
    const mediaFlag = isMediaMode || isLikelyVideoGrabUrl(cleanUrl);
    setIsMediaMode(mediaFlag);
    setProbing(true);
    try {
      if (mediaFlag) {
        const res = await mediaProbe(cleanUrl, headers.length > 0 ? headers : undefined);
        setMediaProbeResult(res);
        setProbeResult(null);
        if (res.title) {
          setFileName(`${res.title}.${selectedFormatId === "audio_only" ? "mp3" : "mp4"}`);
        }
        setCategoryId(selectedFormatId === "audio_only" ? "music" : "video");
      } else {
        const res = await probeUrl(cleanUrl);
        setProbeResult(res);
        setMediaProbeResult(null);
        setFileName(res.file_name);
      }
    } catch (e) {
      console.error(e);
      alert("Gagal memeriksa URL: " + e);
    } finally {
      setProbing(false);
    }
  };

  const handleSubmit = async (startNow: boolean) => {
    if (!url.trim()) return;
    setSubmitting(true);
    try {
      const cleanUrl = url.trim();
      const useMedia = isMediaMode || isLikelyVideoGrabUrl(cleanUrl);
      const audioOnly = selectedFormatId === "audio_only";
      const formatId =
        selectedFormatId !== "best" && selectedFormatId !== "audio_only"
          ? selectedFormatId
          : undefined;

      await addDownload({
        url: cleanUrl,
        file_name: fileName.trim() || undefined,
        save_dir: saveDir.trim() || undefined,
        category_id: categoryId || (useMedia ? (audioOnly ? "music" : "video") : undefined),
        queue_id: queueId || undefined,
        segments_cfg: segments,
        headers: headers.length > 0 ? headers : undefined,
        start_now: startNow,
        is_media: useMedia,
        format_id: formatId,
        audio_only: audioOnly,
      });
      setAddModalOpen(false);
      setPendingPrompt(null);
      setUrl("");
      setProbeResult(null);
      setMediaProbeResult(null);
      setIsMediaMode(false);
      setSelectedFormatId("best");
      setFileName("");
      setCategoryId("");
      setQueueId("");
      setHeaders([]);
    } catch (e) {
      alert("Gagal menambah unduhan: " + e);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
      <div className="w-full max-w-xl rounded-xl border border-tg-outline/40 bg-tg-card shadow-2xl overflow-hidden text-xs">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-tg-outline/30 px-4 py-3 bg-tg-low">
          <div className="flex items-center gap-2.5 text-tg-ink font-semibold">
            <span className="flex h-7 w-7 items-center justify-center rounded-lg bg-tg-primary-container/20 text-tg-primary">
              <Download className="h-4 w-4" />
            </span>
            <div>
              <div className="text-sm leading-tight">
                {pendingPrompt ? "Konfirmasi Unduhan dari Browser" : t("dialog_add_title")}
              </div>
              <div className="text-[10px] font-normal text-tg-muted">
                HTTP/2 • FTP/FTPS • BitTorrent Magnet / .torrent
              </div>
            </div>
          </div>
          <button
            onClick={handleClose}
            className="rounded-md p-1 text-tg-muted hover:bg-tg-high hover:text-tg-ink"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* Form Body */}
        <div className="p-4 space-y-3.5 text-tg-ink">
          {pendingPrompt && (
            <div className="rounded-lg border border-tg-secondary/40 bg-tg-secondary/10 px-3 py-2 text-[11px] text-tg-secondary">
              Unduhan baru terdeteksi dari browser. Pilih folder tujuan lalu klik{" "}
              <strong>{t("btn_start_now")}</strong> untuk memulai.
            </div>
          )}

          {/* URL Input with Protocol Badge */}
          <div>
            <div className="flex items-center justify-between mb-1">
              <label className="block text-tg-ink font-medium">
                {t("dialog_url_label")} (HTTP / FTP / Magnet / .torrent)
              </label>
              <div className="flex items-center gap-1 text-[10px]">
                <button
                  type="button"
                  onClick={handlePasteClipboard}
                  className="inline-flex items-center gap-1 rounded bg-tg-high px-2 py-0.5 text-tg-muted hover:text-tg-ink"
                >
                  <Clipboard className="h-3 w-3" />
                  Paste
                </button>
                {url && (
                  <button
                    type="button"
                    onClick={() => {
                      setUrl("");
                      setProbeResult(null);
                    }}
                    className="inline-flex items-center gap-1 rounded bg-tg-high px-2 py-0.5 text-tg-muted hover:text-tg-error"
                  >
                    <Trash2 className="h-3 w-3" />
                    Clear
                  </button>
                )}
              </div>
            </div>
            <div className="flex gap-1.5">
              <div className="flex flex-1 items-center rounded-lg border border-tg-outline/40 bg-tg-lowest overflow-hidden focus-within:border-tg-primary">
                <span className="bg-tg-high px-2.5 py-2 font-mono text-[10px] font-semibold text-tg-primary border-r border-tg-outline/30">
                  {detectProtocolBadge(url)}
                </span>
                <input
                  type="text"
                  value={url}
                  onChange={(e) => {
                    const nextUrl = e.target.value;
                    setUrl(nextUrl);
                    setIsMediaMode(isLikelyVideoGrabUrl(nextUrl));
                  }}
                  placeholder="https://example.com/file.zip atau link YouTube/TikTok/.m3u8"
                  className="flex-1 bg-transparent px-3 py-1.5 text-xs text-tg-ink placeholder-tg-muted/60 focus:outline-none font-mono"
                />
              </div>
              <button
                type="button"
                onClick={handlePickTorrent}
                disabled={probing}
                title="Pilih file .torrent lokal"
                className="flex items-center gap-1 rounded-lg bg-tg-high px-2.5 py-1.5 font-medium text-tg-ink border border-tg-outline/40 hover:bg-tg-highest disabled:opacity-50"
              >
                <FileUp className="h-3.5 w-3.5 text-tg-tertiary" />
                .torrent
              </button>
              <button
                onClick={handleProbe}
                disabled={probing || !url.trim()}
                className="flex items-center gap-1 rounded-lg bg-tg-primary-container/20 px-3 py-1.5 font-semibold text-tg-primary border border-tg-primary/40 hover:bg-tg-primary-container/30 disabled:opacity-50"
              >
                <Search className="h-3.5 w-3.5" />
                {probing ? t("dialog_probing") : t("dialog_probe_btn")}
              </button>
            </div>
          </div>

          {/* Media / Video Quality Card */}
          {(isMediaMode || mediaProbeResult) && (
            <div className="rounded-lg border border-tg-primary/40 bg-tg-primary-container/10 p-3 space-y-2">
              <div className="flex items-center justify-between">
                <span className="inline-flex items-center gap-1.5 font-semibold text-tg-primary">
                  <Film className="h-3.5 w-3.5" />
                  Mode Video Grabber ({mediaProbeResult?.extractor?.toUpperCase() || "MEDIA"})
                </span>
                {mediaProbeResult?.duration ? (
                  <span className="font-mono text-[10px] text-tg-muted">
                    {Math.floor(mediaProbeResult.duration / 60)}m {Math.round(mediaProbeResult.duration % 60)}s
                  </span>
                ) : null}
              </div>
              <select
                value={selectedFormatId}
                onChange={(e) => setSelectedFormatId(e.target.value)}
                className="w-full rounded-lg border border-tg-outline/40 bg-tg-lowest px-2.5 py-1.5 text-xs text-tg-ink"
              >
                <option value="best">Kualitas Terbaik (Auto Merge MP4)</option>
                <option value="audio_only">Audio Saja (Konversi MP3)</option>
                {prepareVideoFormatOptions(mediaProbeResult?.formats || []).map((opt) => (
                  <option key={opt.formatId} value={opt.submitValue}>
                    {opt.resolutionLabel} [{opt.extLabel} • {opt.codecLabel}
                    {opt.needsAudioMerge ? " +Audio" : ""}] — {opt.sizeSummary}
                  </option>
                ))}
                {prepareAudioFormatOptions(mediaProbeResult?.formats || []).map((opt) => (
                  <option key={opt.formatId} value={opt.submitValue}>
                    {opt.resolutionLabel} [{opt.extLabel} • {opt.codecLabel} • {opt.bitrateLabel}] — {opt.sizeSummary}
                  </option>
                ))}
              </select>
            </div>
          )}

          {/* Probe Info Card */}
          {probeResult && (
            <div className="rounded-lg border border-tg-outline/30 bg-tg-lowest p-3 space-y-1.5">
              <div className="flex justify-between items-center">
                <span className="text-tg-muted">Ukuran Terdeteksi:</span>
                <span className="font-mono text-tg-ink font-semibold">
                  {formatBytes(probeResult.total_bytes)}
                </span>
              </div>
              <div className="flex justify-between items-center">
                <span className="text-tg-muted">Dukungan Resume:</span>
                {probeResult.resumable ? (
                  <span className="inline-flex items-center gap-1 text-tg-secondary text-[11px] font-medium">
                    <CheckCircle className="h-3.5 w-3.5" />
                    {t("dialog_resumable")}
                  </span>
                ) : (
                  <span className="inline-flex items-center gap-1 text-tg-tertiary text-[11px] font-medium">
                    <AlertTriangle className="h-3.5 w-3.5" />
                    {t("dialog_non_resumable")}
                  </span>
                )}
              </div>
            </div>
          )}

          {/* File Name & Category Row */}
          <div className="grid grid-cols-1 sm:grid-cols-3 gap-2.5">
            <div className="sm:col-span-2">
              <label className="block text-tg-ink font-medium mb-1">{t("dialog_filename")}</label>
              <input
                type="text"
                value={fileName}
                onChange={(e) => setFileName(e.target.value)}
                placeholder="nama_file.zip"
                className="w-full rounded-lg border border-tg-outline/40 bg-tg-lowest px-3 py-1.5 text-xs text-tg-ink placeholder-tg-muted/60 focus:border-tg-primary focus:outline-none"
              />
            </div>
            <div>
              <label className="block text-tg-ink font-medium mb-1">{t("col_category")}</label>
              <select
                value={categoryId}
                onChange={(e) => setCategoryId(e.target.value)}
                className="w-full rounded-lg border border-tg-outline/40 bg-tg-lowest px-2.5 py-1.5 text-xs text-tg-ink focus:border-tg-primary focus:outline-none"
              >
                <option value="">Otomatis</option>
                <option value="compressed">Compressed</option>
                <option value="documents">Dokumen</option>
                <option value="music">Musik</option>
                <option value="programs">Program</option>
                <option value="video">Video</option>
                <option value="images">Gambar</option>
                <option value="torrent">Torrent</option>
                <option value="others">Lainnya</option>
              </select>
            </div>
          </div>

          {/* Save Dir + Queue */}
          <div className="grid grid-cols-1 sm:grid-cols-3 gap-2.5">
            <div className="sm:col-span-2">
              <label className="block text-tg-ink font-medium mb-1">{t("dialog_save_dir")}</label>
              <div className="flex gap-1.5">
                <input
                  type="text"
                  value={saveDir}
                  onChange={(e) => setSaveDir(e.target.value)}
                  className="flex-1 rounded-lg border border-tg-outline/40 bg-tg-lowest px-3 py-1.5 text-xs text-tg-ink font-mono focus:border-tg-primary focus:outline-none"
                />
                <button
                  type="button"
                  onClick={handleBrowseFolder}
                  title="Pilih Folder Simpan"
                  className="rounded-lg border border-tg-outline/40 bg-tg-high px-2.5 py-1.5 text-tg-ink hover:bg-tg-highest"
                >
                  <Folder className="h-4 w-4 text-tg-tertiary" />
                </button>
              </div>
            </div>
            <div>
              <label className="block text-tg-ink font-medium mb-1">Antrean</label>
              <select
                value={queueId}
                onChange={(e) => setQueueId(e.target.value)}
                className="w-full rounded-lg border border-tg-outline/40 bg-tg-lowest px-2.5 py-1.5 text-xs text-tg-ink focus:border-tg-primary focus:outline-none"
              >
                <option value="">Antrean Utama</option>
                {queues.map((q) => (
                  <option key={q.id} value={q.id}>
                    {q.name}
                  </option>
                ))}
              </select>
            </div>
          </div>

          {/* Segments configuration */}
          <div className="rounded-lg border border-tg-outline/30 bg-tg-lowest/70 p-2.5">
            <div className="flex justify-between items-center mb-1">
              <label className="text-tg-ink font-medium">{t("dialog_segments")}</label>
              <span className="font-mono text-tg-secondary font-bold">{segments} Segmen</span>
            </div>
            <input
              type="range"
              min={1}
              max={32}
              value={segments}
              onChange={(e) => setSegments(parseInt(e.target.value, 10))}
              className="w-full accent-tg-primary-container"
            />
          </div>
        </div>

        {/* Footer Actions */}
        <div className="flex items-center justify-end gap-2 border-t border-tg-outline/30 bg-tg-low px-4 py-3">
          <button
            onClick={handleClose}
            className="rounded-lg px-3 py-1.5 text-tg-muted hover:bg-tg-high hover:text-tg-ink"
          >
            {t("btn_cancel")}
          </button>
          <button
            onClick={() => handleSubmit(false)}
            disabled={submitting || !url.trim()}
            className="rounded-lg border border-tg-outline/40 bg-tg-high px-3 py-1.5 font-medium text-tg-ink hover:bg-tg-highest disabled:opacity-50"
          >
            {t("btn_download_later")}
          </button>
          <button
            onClick={() => handleSubmit(true)}
            disabled={submitting || !url.trim()}
            className="rounded-lg bg-tg-primary-container px-4 py-1.5 font-semibold text-white shadow hover:brightness-110 active:scale-95 disabled:opacity-50"
          >
            {t("btn_start_now")}
          </button>
        </div>
      </div>
    </div>
  );
};
