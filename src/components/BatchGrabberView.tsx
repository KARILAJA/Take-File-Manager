import React, { useEffect, useState } from "react";
import {
  Radar,
  Play,
  Square,
  Download,
  FileSpreadsheet,
  Sparkles,
  CheckSquare,
  ArrowLeft,
  Folder,
  Copy,
  Film,
  Search,
  CheckCircle,
  Clipboard,
} from "lucide-react";
import { useAppStore, CrawledItem, MediaProbeResult, MediaToolsStatus } from "../store";
import { formatBytes } from "../utils/format";
import { prepareVideoFormatOptions, prepareAudioFormatOptions } from "../utils/mediaFormats";

export const BatchGrabberView: React.FC = () => {
  const {
    queues,
    setActiveView,
    getDefaultDownloadDir,
    pickFolder,
    expandUrlPattern,
    addBatch,
    addFromPattern,
    mediaProbe,
    mediaDownload,
    getMediaToolsStatus,
    grabSiteStart,
    grabSiteStop,
    exportDownloadsList,
    importDownloadsList,
  } = useAppStore();

  const [tab, setTab] = useState<"batch" | "video" | "crawler" | "transfer">("batch");
  const [saveDir, setSaveDir] = useState("");
  const [queueId, setQueueId] = useState("");
  const [segments, setSegments] = useState(8);
  const [startNow, setStartNow] = useState(true);

  // Video Grabber state
  const [videoUrl, setVideoUrl] = useState("");
  const [probingVideo, setProbingVideo] = useState(false);
  const [videoProbe, setVideoProbe] = useState<MediaProbeResult | null>(null);
  const [videoFormatId, setVideoFormatId] = useState("best");
  const [videoFileName, setVideoFileName] = useState("");
  const [toolsStatus, setToolsStatus] = useState<MediaToolsStatus | null>(null);

  // Batch & Pattern state
  const [patternOrUrls, setPatternOrUrls] = useState("");
  const [previewUrls, setPreviewUrls] = useState<string[]>([]);
  const [batchNotice, setBatchNotice] = useState<string | null>(null);

  // Crawler state
  const [startUrl, setStartUrl] = useState("");
  const [maxDepth, setMaxDepth] = useState(1);
  const [extFilter, setExtFilter] = useState("zip,pdf,mp4,exe,iso");
  const [includePat, setIncludePat] = useState("");
  const [excludePat, setExcludePat] = useState("");
  const [crawling, setCrawling] = useState(false);
  const [crawledItems, setCrawledItems] = useState<CrawledItem[]>([]);
  const [selectedCrawlUrls, setSelectedCrawlUrls] = useState<string[]>([]);

  // Import/Export state
  const [transferFormat, setTransferFormat] = useState<"txt" | "csv" | "json">("txt");
  const [transferText, setTransferText] = useState("");

  useEffect(() => {
    getDefaultDownloadDir().then((dir) => {
      if (dir) setSaveDir(dir);
    });
    getMediaToolsStatus().then(setToolsStatus).catch(() => {});
  }, [getDefaultDownloadDir, getMediaToolsStatus]);

  const handleAnalyzeVideo = async () => {
    if (!videoUrl.trim()) return;
    setProbingVideo(true);
    setBatchNotice(null);
    try {
      const res = await mediaProbe(videoUrl.trim());
      setVideoProbe(res);
      setVideoFileName(res.title || "");
      setVideoFormatId("best");
      getMediaToolsStatus().then(setToolsStatus).catch(() => {});
      setBatchNotice(`Video terdeteksi: "${res.title}" (${res.formats.length} format tersedia).`);
    } catch (e) {
      setBatchNotice(`Gagal menganalisis video: ${String(e)}`);
    } finally {
      setProbingVideo(false);
    }
  };

  const handleDownloadVideo = async () => {
    if (!videoUrl.trim()) return;
    const isAudio = videoFormatId === "audio_only";
    const fid =
      videoFormatId !== "best" && videoFormatId !== "audio_only" ? videoFormatId : undefined;
    try {
      await mediaDownload({
        url: videoUrl.trim(),
        format_id: fid,
        audio_only: isAudio,
        save_dir: saveDir || undefined,
        file_name: videoFileName.trim() || undefined,
        queue_id: queueId || undefined,
        concurrency: Math.min(16, Math.max(1, segments)),
      });
      setBatchNotice("Video ditambahkan ke daftar unduhan! Mengalihkan ke Tasks...");
      setTimeout(() => setActiveView("downloads"), 500);
    } catch (e) {
      setBatchNotice(`Gagal mengunduh video: ${String(e)}`);
    }
  };

  const handleBrowse = async () => {
    const dir = await pickFolder(saveDir || undefined);
    if (dir) setSaveDir(dir);
  };

  const handlePreviewPattern = async () => {
    const line = patternOrUrls.trim().split(/\r?\n/)[0] || "";
    if (!line) return;
    try {
      const expanded = await expandUrlPattern(line);
      setPreviewUrls(expanded);
      setBatchNotice(`${expanded.length} URL siap dari pola.`);
    } catch (e) {
      setBatchNotice(`Pola tidak valid: ${String(e)}`);
    }
  };

  const handleStartBatch = async () => {
    if (!patternOrUrls.trim()) return;
    try {
      const hasRange = /\[\d+-\d+\]/.test(patternOrUrls);
      const ids = hasRange
        ? await addFromPattern({
            urls_or_text: patternOrUrls.trim(),
            save_dir: saveDir || undefined,
            queue_id: queueId || undefined,
            segments_cfg: segments,
            start_now: startNow,
          })
        : await addBatch({
            urls_or_text: patternOrUrls.trim(),
            save_dir: saveDir || undefined,
            queue_id: queueId || undefined,
            segments_cfg: segments,
            start_now: startNow,
          });
      setBatchNotice(`Berhasil menambahkan ${ids.length} unduhan batch!`);
    } catch (e) {
      setBatchNotice(`Gagal menambahkan batch: ${String(e)}`);
    }
  };

  const handleStartCrawl = async () => {
    if (!startUrl.trim()) return;
    setCrawling(true);
    setBatchNotice(null);
    try {
      let domain = "";
      try {
        domain = new URL(startUrl.trim()).hostname;
      } catch {
        domain = "";
      }
      const exts = extFilter
        .split(",")
        .map((s) => s.trim().toLowerCase())
        .filter(Boolean);
      const items = await grabSiteStart({
        start_url: startUrl.trim(),
        max_depth: maxDepth,
        allowed_domains: domain ? [domain] : [],
        include_subdomains: true,
        file_extensions: exts,
        include_patterns: includePat
          .split(",")
          .map((s) => s.trim())
          .filter(Boolean),
        exclude_patterns: excludePat
          .split(",")
          .map((s) => s.trim())
          .filter(Boolean),
        max_pages: 60,
      });
      setCrawledItems(items);
      setSelectedCrawlUrls(items.map((i) => i.url));
      setBatchNotice(`Ditemukan ${items.length} file dari situs target.`);
    } catch (e) {
      setBatchNotice(`Crawler error: ${String(e)}`);
    } finally {
      setCrawling(false);
    }
  };

  const handleDownloadCrawled = async () => {
    if (selectedCrawlUrls.length === 0) return;
    const ids = await addBatch({
      urls_or_text: selectedCrawlUrls.join("\n"),
      save_dir: saveDir || undefined,
      queue_id: queueId || undefined,
      segments_cfg: segments,
      start_now: startNow,
    });
    setBatchNotice(`${ids.length} item hasil grab ditambahkan ke daftar unduhan.`);
  };

  const handleExport = async () => {
    const out = await exportDownloadsList(transferFormat);
    setTransferText(out);
    setBatchNotice(`Ekspor format ${transferFormat.toUpperCase()} selesai.`);
  };

  const handleImport = async () => {
    if (!transferText.trim()) return;
    const ids = await importDownloadsList(
      transferFormat,
      transferText,
      saveDir || undefined,
      startNow
    );
    setBatchNotice(`Berhasil mengimpor ${ids.length} URL unduhan.`);
  };

  return (
    <div className="flex flex-1 flex-col overflow-hidden bg-tg-bg p-4 text-xs">
      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-tg-outline/30 pb-3 mb-3">
        <div className="flex items-center gap-3">
          <button
            onClick={() => setActiveView("downloads")}
            className="flex items-center gap-1 rounded-lg border border-tg-outline/40 bg-tg-card px-2.5 py-1.5 text-tg-ink hover:bg-tg-high"
          >
            <ArrowLeft className="h-3.5 w-3.5" />
            <span>Kembali</span>
          </button>
          <div>
            <h1 className="text-sm font-bold text-tg-ink flex items-center gap-2">
              <Radar className="h-4 w-4 text-tg-primary" />
              Batch Downloader &amp; Site Grabber
            </h1>
            <p className="text-[11px] text-tg-muted">
              Ekspansi pola URL otomatis [001-100], crawler situs web terfilter, &amp; ekspor/impor antrean.
            </p>
          </div>
        </div>

        <div className="flex items-center gap-1 rounded-lg border border-tg-outline/40 bg-tg-lowest p-1">
          <button
            onClick={() => setTab("video")}
            className={`flex items-center gap-1.5 rounded-md px-3 py-1.5 font-medium transition ${
              tab === "video" ? "bg-tg-primary-container text-white" : "text-tg-muted hover:text-tg-ink"
            }`}
          >
            <Film className="h-3.5 w-3.5" />
            Video Grabber
          </button>
          <button
            onClick={() => setTab("batch")}
            className={`rounded-md px-3 py-1.5 font-medium transition ${
              tab === "batch" ? "bg-tg-primary-container text-white" : "text-tg-muted hover:text-tg-ink"
            }`}
          >
            Batch &amp; Pola URL
          </button>
          <button
            onClick={() => setTab("crawler")}
            className={`rounded-md px-3 py-1.5 font-medium transition ${
              tab === "crawler" ? "bg-tg-primary-container text-white" : "text-tg-muted hover:text-tg-ink"
            }`}
          >
            Site Grabber
          </button>
          <button
            onClick={() => setTab("transfer")}
            className={`rounded-md px-3 py-1.5 font-medium transition ${
              tab === "transfer" ? "bg-tg-primary-container text-white" : "text-tg-muted hover:text-tg-ink"
            }`}
          >
            Impor / Ekspor
          </button>
        </div>
      </div>

      {batchNotice && (
        <div className="mb-3 rounded-lg border border-tg-primary/40 bg-tg-primary-container/15 px-3 py-2 text-tg-primary font-medium">
          {batchNotice}
        </div>
      )}

      {/* Common Save Options Bar */}
      <div className="grid grid-cols-1 md:grid-cols-12 gap-2.5 rounded-xl border border-tg-outline/30 bg-tg-card p-3 mb-3">
        <div className="md:col-span-5">
          <label className="block text-[11px] text-tg-muted mb-1">Folder Simpan Tujuan</label>
          <div className="flex gap-1.5">
            <input
              type="text"
              value={saveDir}
              onChange={(e) => setSaveDir(e.target.value)}
              className="flex-1 rounded-lg border border-tg-outline/40 bg-tg-lowest px-2.5 py-1.5 font-mono text-tg-ink"
            />
            <button
              onClick={handleBrowse}
              className="rounded-lg border border-tg-outline/40 bg-tg-high px-2.5 py-1.5 hover:bg-tg-highest"
            >
              <Folder className="h-4 w-4 text-tg-tertiary" />
            </button>
          </div>
        </div>

        <div className="md:col-span-3">
          <label className="block text-[11px] text-tg-muted mb-1">Antrean Target</label>
          <select
            value={queueId}
            onChange={(e) => setQueueId(e.target.value)}
            className="w-full rounded-lg border border-tg-outline/40 bg-tg-lowest px-2.5 py-1.5 text-tg-ink"
          >
            <option value="">Antrean Utama</option>
            {queues.map((q) => (
              <option key={q.id} value={q.id}>
                {q.name}
              </option>
            ))}
          </select>
        </div>

        <div className="md:col-span-2">
          <label className="block text-[11px] text-tg-muted mb-1">Segmen</label>
          <input
            type="number"
            min={1}
            max={32}
            value={segments}
            onChange={(e) => setSegments(Math.max(1, Math.min(32, parseInt(e.target.value || "8", 10) || 8)))}
            className="w-full rounded-lg border border-tg-outline/40 bg-tg-lowest px-2.5 py-1.5 font-mono text-tg-ink"
          />
        </div>

        <div className="md:col-span-2 flex items-end pb-1.5">
          <label className="flex items-center gap-2 text-tg-ink cursor-pointer">
            <input
              type="checkbox"
              checked={startNow}
              onChange={(e) => setStartNow(e.target.checked)}
              className="accent-tg-primary-container"
            />
            Mulai langsung
          </label>
        </div>
      </div>

      {/* Body */}
      <div className="flex-1 overflow-auto rounded-xl border border-tg-outline/30 bg-tg-card p-4">
        {tab === "video" && (
          <div className="space-y-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <h2 className="font-semibold text-tg-ink flex items-center gap-2">
                  <Film className="h-4 w-4 text-tg-primary" />
                  Video &amp; Streaming Grabber (YouTube / TikTok / IG / HLS / DASH / MP4)
                </h2>
                <p className="text-[11px] text-tg-muted mt-0.5">
                  Tempel URL halaman video atau stream .m3u8/.mpd untuk memilih resolusi dan mengunduh seperti IDM.
                </p>
              </div>
              {toolsStatus && (
                <div className="flex items-center gap-2 text-[10px] font-mono">
                  <span
                    className={`inline-flex items-center gap-1 rounded px-2 py-0.5 border ${
                      toolsStatus.ytdlp_installed
                        ? "border-tg-secondary/40 bg-tg-secondary/10 text-tg-secondary"
                        : "border-tg-tertiary/40 bg-tg-tertiary/10 text-tg-tertiary"
                    }`}
                  >
                    <CheckCircle className="h-3 w-3" />
                    yt-dlp: {toolsStatus.ytdlp_installed ? "Siap" : "Auto-unduh saat analisis"}
                  </span>
                  <span
                    className={`inline-flex items-center gap-1 rounded px-2 py-0.5 border ${
                      toolsStatus.ffmpeg_installed
                        ? "border-tg-secondary/40 bg-tg-secondary/10 text-tg-secondary"
                        : "border-tg-error/40 bg-tg-error/10 text-tg-error"
                    }`}
                  >
                    ffmpeg: {toolsStatus.ffmpeg_installed ? "Siap" : "Belum ada"}
                  </span>
                </div>
              )}
            </div>

            <div className="flex gap-2">
              <div className="flex flex-1 items-center rounded-lg border border-tg-outline/40 bg-tg-lowest overflow-hidden focus-within:border-tg-primary">
                <input
                  type="text"
                  value={videoUrl}
                  onChange={(e) => setVideoUrl(e.target.value)}
                  placeholder="https://www.youtube.com/watch?v=... atau https://.../playlist.m3u8"
                  className="flex-1 bg-transparent px-3 py-2 font-mono text-xs text-tg-ink focus:outline-none"
                />
                <button
                  type="button"
                  onClick={async () => {
                    try {
                      if (navigator.clipboard?.readText) {
                        const txt = await navigator.clipboard.readText();
                        if (txt) setVideoUrl(txt.trim());
                      }
                    } catch {
                      // Clipboard unavailable
                    }
                  }}
                  className="px-2.5 py-2 text-tg-muted hover:text-tg-ink border-l border-tg-outline/30"
                  title="Tempel dari Clipboard"
                >
                  <Clipboard className="h-3.5 w-3.5" />
                </button>
              </div>
              <button
                type="button"
                onClick={handleAnalyzeVideo}
                disabled={probingVideo || !videoUrl.trim()}
                className="flex items-center gap-1.5 rounded-lg bg-tg-high px-4 py-2 font-semibold text-tg-ink border border-tg-outline/40 hover:bg-tg-highest disabled:opacity-50"
              >
                <Search className="h-3.5 w-3.5 text-tg-primary" />
                {probingVideo ? "Menganalisis..." : "Analisis Video"}
              </button>
              <button
                type="button"
                onClick={handleDownloadVideo}
                disabled={probingVideo || !videoUrl.trim()}
                className="flex items-center gap-1.5 rounded-lg bg-tg-primary-container px-4 py-2 font-semibold text-white hover:brightness-110 disabled:opacity-50"
              >
                <Download className="h-3.5 w-3.5" />
                Unduh Video
              </button>
            </div>

            {videoProbe && (
              <div className="grid grid-cols-1 md:grid-cols-12 gap-4 rounded-xl border border-tg-outline/30 bg-tg-lowest p-3.5">
                {videoProbe.thumbnail && (
                  <div className="md:col-span-4 overflow-hidden rounded-lg border border-tg-outline/30 bg-tg-card">
                    <img
                      src={videoProbe.thumbnail}
                      alt={videoProbe.title}
                      className="w-full h-36 object-cover"
                    />
                  </div>
                )}
                <div className={`${videoProbe.thumbnail ? "md:col-span-8" : "md:col-span-12"} space-y-3`}>
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <span className="font-bold text-sm text-tg-ink truncate">{videoProbe.title}</span>
                    <div className="flex items-center gap-2 font-mono text-[10px]">
                      {videoProbe.extractor && (
                        <span className="rounded bg-tg-primary-container/20 px-2 py-0.5 text-tg-primary uppercase">
                          {videoProbe.extractor}
                        </span>
                      )}
                      {videoProbe.duration && (
                        <span className="text-tg-muted">
                          {Math.floor(videoProbe.duration / 60)}m {Math.round(videoProbe.duration % 60)}s
                        </span>
                      )}
                    </div>
                  </div>

                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
                    <div>
                      <label className="block text-[11px] text-tg-muted mb-1">Nama File Output</label>
                      <input
                        type="text"
                        value={videoFileName}
                        onChange={(e) => setVideoFileName(e.target.value)}
                        placeholder="judul_video"
                        className="w-full rounded-lg border border-tg-outline/40 bg-tg-card px-2.5 py-1.5 text-tg-ink"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-tg-muted mb-1">Kualitas / Resolusi</label>
                      <select
                        value={videoFormatId}
                        onChange={(e) => setVideoFormatId(e.target.value)}
                        className="w-full rounded-lg border border-tg-outline/40 bg-tg-card px-2.5 py-1.5 text-tg-ink"
                      >
                        <option value="best">Kualitas Terbaik (Auto Merge MP4)</option>
                        <option value="audio_only">Audio Saja (Konversi MP3)</option>
                        {prepareVideoFormatOptions(videoProbe.formats).map((opt) => (
                          <option key={opt.formatId} value={opt.submitValue}>
                            {opt.resolutionLabel} [{opt.extLabel} • {opt.codecLabel}
                            {opt.needsAudioMerge ? " +Audio" : ""}] — {opt.sizeSummary}
                          </option>
                        ))}
                        {prepareAudioFormatOptions(videoProbe.formats).map((opt) => (
                          <option key={opt.formatId} value={opt.submitValue}>
                            {opt.resolutionLabel} [{opt.extLabel} • {opt.codecLabel} • {opt.bitrateLabel}] — {opt.sizeSummary}
                          </option>
                        ))}
                      </select>
                    </div>
                  </div>

                  {/* Detailed table of ALL available video & audio resolutions */}
                  <div className="rounded-lg border border-tg-outline/30 bg-tg-card overflow-hidden max-h-56 overflow-y-auto">
                    <table className="w-full text-left border-collapse text-[11px]">
                      <thead className="bg-tg-low text-tg-muted border-b border-tg-outline/30 sticky top-0">
                        <tr>
                          <th className="px-2.5 py-1.5">Resolusi / Track</th>
                          <th className="px-2 py-1.5">Format &amp; Codec</th>
                          <th className="px-2 py-1.5">Bitrate / FPS</th>
                          <th className="px-2 py-1.5">Ukuran Detail</th>
                          <th className="px-2 py-1.5 text-right">Aksi</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-tg-outline/20">
                        {prepareVideoFormatOptions(videoProbe.formats).map((opt) => (
                          <tr
                            key={`row-${opt.formatId}`}
                            className={`hover:bg-tg-high/50 ${
                              videoFormatId === opt.submitValue ? "bg-tg-primary-container/15" : ""
                            }`}
                          >
                            <td className="px-2.5 py-1.5 font-semibold text-tg-ink">
                              {opt.resolutionLabel}
                            </td>
                            <td className="px-2 py-1.5 font-mono text-tg-muted">
                              {opt.extLabel} • {opt.codecLabel}
                              {opt.needsAudioMerge ? " (+Audio)" : ""}
                            </td>
                            <td className="px-2 py-1.5 font-mono text-tg-muted">
                              {opt.bitrateLabel} • {opt.fpsLabel}
                            </td>
                            <td className="px-2 py-1.5 font-mono text-tg-secondary font-medium">
                              {opt.sizeSummary}
                            </td>
                            <td className="px-2 py-1.5 text-right">
                              <button
                                type="button"
                                onClick={() => setVideoFormatId(opt.submitValue)}
                                className="rounded bg-tg-high px-2 py-0.5 text-[10px] font-semibold text-tg-primary hover:bg-tg-highest"
                              >
                                Pilih
                              </button>
                            </td>
                          </tr>
                        ))}
                        {prepareAudioFormatOptions(videoProbe.formats).map((opt) => (
                          <tr
                            key={`aud-${opt.formatId}`}
                            className={`hover:bg-tg-high/50 ${
                              videoFormatId === opt.submitValue ? "bg-tg-primary-container/15" : ""
                            }`}
                          >
                            <td className="px-2.5 py-1.5 font-semibold text-tg-tertiary">
                              {opt.resolutionLabel}
                            </td>
                            <td className="px-2 py-1.5 font-mono text-tg-muted">
                              {opt.extLabel} • {opt.codecLabel}
                            </td>
                            <td className="px-2 py-1.5 font-mono text-tg-muted">
                              {opt.bitrateLabel}
                            </td>
                            <td className="px-2 py-1.5 font-mono text-tg-secondary font-medium">
                              {opt.sizeSummary}
                            </td>
                            <td className="px-2 py-1.5 text-right">
                              <button
                                type="button"
                                onClick={() => setVideoFormatId(opt.submitValue)}
                                className="rounded bg-tg-high px-2 py-0.5 text-[10px] font-semibold text-tg-primary hover:bg-tg-highest"
                              >
                                Pilih
                              </button>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </div>
              </div>
            )}
          </div>
        )}

        {tab === "batch" && (
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-4 h-full">
            <div className="flex flex-col space-y-2.5">
              <label className="font-semibold text-tg-ink">
                Masukkan Daftar URL (satu per baris) atau Pola Wildcard Range:
              </label>
              <p className="text-[11px] text-tg-muted font-mono">
                Contoh pola: https://cdn.example.com/archive/part-[001-010].zip
              </p>
              <textarea
                rows={10}
                value={patternOrUrls}
                onChange={(e) => setPatternOrUrls(e.target.value)}
                placeholder={"https://example.com/files/episode-[01-05].mp4\natau tempel banyak URL per baris..."}
                className="flex-1 w-full rounded-lg border border-tg-outline/40 bg-tg-lowest p-3 font-mono text-xs text-tg-ink focus:border-tg-primary focus:outline-none"
              />
              <div className="flex items-center gap-2">
                <button
                  onClick={handlePreviewPattern}
                  className="flex items-center gap-1.5 rounded-lg border border-tg-outline/40 bg-tg-high px-3 py-2 font-medium text-tg-ink hover:bg-tg-highest"
                >
                  <Sparkles className="h-3.5 w-3.5 text-tg-tertiary" />
                  Pratinjau Pola
                </button>
                <button
                  onClick={handleStartBatch}
                  disabled={!patternOrUrls.trim()}
                  className="flex items-center gap-1.5 rounded-lg bg-tg-primary-container px-4 py-2 font-semibold text-white hover:brightness-110 disabled:opacity-50"
                >
                  <Download className="h-3.5 w-3.5" />
                  Tambahkan Batch Unduhan
                </button>
              </div>
            </div>

            <div className="flex flex-col rounded-lg border border-tg-outline/30 bg-tg-lowest p-3">
              <div className="font-semibold text-tg-ink mb-2 flex justify-between">
                <span>Pratinjau URL Terjabarkan</span>
                <span className="font-mono text-tg-secondary">{previewUrls.length} URL</span>
              </div>
              <div className="flex-1 overflow-auto space-y-1 font-mono text-[11px] text-tg-muted">
                {previewUrls.length === 0 ? (
                  <p className="py-8 text-center text-tg-muted/70">
                    Klik &quot;Pratinjau Pola&quot; untuk melihat daftar URL hasil ekspansi.
                  </p>
                ) : (
                  previewUrls.map((u, i) => (
                    <div key={i} className="truncate rounded bg-tg-card/60 px-2 py-1 text-tg-ink">
                      {i + 1}. {u}
                    </div>
                  ))
                )}
              </div>
            </div>
          </div>
        )}

        {tab === "crawler" && (
          <div className="space-y-3">
            <div className="grid grid-cols-1 md:grid-cols-12 gap-2.5">
              <div className="md:col-span-5">
                <label className="block text-tg-muted mb-1">URL Halaman Awal (Start URL)</label>
                <input
                  type="text"
                  value={startUrl}
                  onChange={(e) => setStartUrl(e.target.value)}
                  placeholder="https://example.com/downloads/"
                  className="w-full rounded-lg border border-tg-outline/40 bg-tg-lowest px-3 py-1.5 font-mono text-tg-ink"
                />
              </div>
              <div className="md:col-span-2">
                <label className="block text-tg-muted mb-1">Kedalaman Crawl</label>
                <input
                  type="number"
                  min={0}
                  max={5}
                  value={maxDepth}
                  onChange={(e) => setMaxDepth(Math.max(0, Math.min(5, parseInt(e.target.value || "1", 10) || 1)))}
                  className="w-full rounded-lg border border-tg-outline/40 bg-tg-lowest px-3 py-1.5 font-mono text-tg-ink"
                />
              </div>
              <div className="md:col-span-5">
                <label className="block text-tg-muted mb-1">Filter Ekstensi (pisah koma)</label>
                <input
                  type="text"
                  value={extFilter}
                  onChange={(e) => setExtFilter(e.target.value)}
                  placeholder="zip,pdf,mp4,exe,iso"
                  className="w-full rounded-lg border border-tg-outline/40 bg-tg-lowest px-3 py-1.5 font-mono text-tg-ink"
                />
              </div>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-2.5">
              <input
                type="text"
                value={includePat}
                onChange={(e) => setIncludePat(e.target.value)}
                placeholder="Include pattern (misal: *release*, *2026*)"
                className="rounded-lg border border-tg-outline/40 bg-tg-lowest px-3 py-1.5 font-mono text-tg-ink"
              />
              <input
                type="text"
                value={excludePat}
                onChange={(e) => setExcludePat(e.target.value)}
                placeholder="Exclude pattern (misal: *thumb*, *beta*)"
                className="rounded-lg border border-tg-outline/40 bg-tg-lowest px-3 py-1.5 font-mono text-tg-ink"
              />
            </div>

            <div className="flex items-center justify-between pt-1">
              <div className="flex items-center gap-2">
                <button
                  onClick={handleStartCrawl}
                  disabled={crawling || !startUrl.trim()}
                  className="flex items-center gap-1.5 rounded-lg bg-tg-primary-container px-4 py-1.5 font-semibold text-white hover:brightness-110 disabled:opacity-50"
                >
                  <Play className="h-3.5 w-3.5" />
                  {crawling ? "Memindai Situs..." : "Mulai Crawl Situs"}
                </button>
                {crawling && (
                  <button
                    onClick={() => grabSiteStop()}
                    className="flex items-center gap-1 rounded-lg bg-tg-error/20 border border-tg-error/40 px-3 py-1.5 text-tg-error"
                  >
                    <Square className="h-3.5 w-3.5" />
                    Stop Crawl
                  </button>
                )}
              </div>

              <button
                onClick={handleDownloadCrawled}
                disabled={selectedCrawlUrls.length === 0}
                className="flex items-center gap-1.5 rounded-lg bg-tg-secondary px-4 py-1.5 font-semibold text-slate-950 hover:brightness-110 disabled:opacity-40"
              >
                <CheckSquare className="h-3.5 w-3.5" />
                Unduh Terpilih ({selectedCrawlUrls.length})
              </button>
            </div>

            {/* Discovered URL Table */}
            <div className="rounded-lg border border-tg-outline/30 bg-tg-lowest overflow-hidden max-h-64 overflow-y-auto">
              <table className="w-full text-left border-collapse text-[11px]">
                <thead className="bg-tg-low text-tg-muted border-b border-tg-outline/30">
                  <tr>
                    <th className="p-2 w-8"></th>
                    <th className="p-2">Nama Berkas</th>
                    <th className="p-2">URL</th>
                    <th className="p-2 w-24">Ukuran</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-tg-outline/20">
                  {crawledItems.length === 0 ? (
                    <tr>
                      <td colSpan={4} className="p-6 text-center text-tg-muted">
                        Masukkan URL situs di atas lalu klik Mulai Crawl Situs.
                      </td>
                    </tr>
                  ) : (
                    crawledItems.map((ci) => {
                      const checked = selectedCrawlUrls.includes(ci.url);
                      return (
                        <tr key={ci.url} className="hover:bg-tg-high/40">
                          <td className="p-2 text-center">
                            <input
                              type="checkbox"
                              checked={checked}
                              onChange={() =>
                                setSelectedCrawlUrls((prev) =>
                                  checked ? prev.filter((u) => u !== ci.url) : [...prev, ci.url]
                                )
                              }
                            />
                          </td>
                          <td className="p-2 font-semibold text-tg-ink">{ci.file_name}</td>
                          <td className="p-2 font-mono text-tg-muted truncate max-w-xs">{ci.url}</td>
                          <td className="p-2 font-mono text-tg-secondary">
                            {formatBytes(ci.content_length ?? undefined)}
                          </td>
                        </tr>
                      );
                    })
                  )}
                </tbody>
              </table>
            </div>
          </div>
        )}

        {tab === "transfer" && (
          <div className="space-y-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                <FileSpreadsheet className="h-4 w-4 text-tg-primary" />
                <span className="font-semibold text-tg-ink">Format Impor / Ekspor Daftar Unduhan</span>
                <select
                  value={transferFormat}
                  onChange={(e) => setTransferFormat(e.target.value as "txt" | "csv" | "json")}
                  className="rounded-lg border border-tg-outline/40 bg-tg-lowest px-2.5 py-1 text-tg-ink"
                >
                  <option value="txt">Plain Text URL (.txt)</option>
                  <option value="csv">Spreadsheet (.csv)</option>
                  <option value="json">Structured (.json)</option>
                </select>
              </div>
              <div className="flex items-center gap-2">
                <button
                  onClick={handleExport}
                  className="flex items-center gap-1 rounded-lg border border-tg-outline/40 bg-tg-high px-3 py-1.5 text-tg-ink hover:bg-tg-highest"
                >
                  <Copy className="h-3.5 w-3.5" />
                  Ekspor Daftar Saat Ini
                </button>
                <button
                  onClick={handleImport}
                  disabled={!transferText.trim()}
                  className="flex items-center gap-1 rounded-lg bg-tg-primary-container px-3 py-1.5 font-semibold text-white hover:brightness-110 disabled:opacity-50"
                >
                  <Download className="h-3.5 w-3.5" />
                  Impor ke Antrean
                </button>
              </div>
            </div>
            <textarea
              rows={11}
              value={transferText}
              onChange={(e) => setTransferText(e.target.value)}
              placeholder="Tempel konten TXT / CSV / JSON di sini untuk diimpor, atau klik Ekspor untuk menyalin daftar unduhan..."
              className="w-full rounded-lg border border-tg-outline/40 bg-tg-lowest p-3 font-mono text-xs text-tg-ink"
            />
          </div>
        )}
      </div>
    </div>
  );
};
