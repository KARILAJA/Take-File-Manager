import React, { useEffect, useState } from "react";
import { X, Layers, Activity, Info, ShieldCheck, FolderOpen } from "lucide-react";
import { useAppStore } from "../store";
import { formatBytes, formatSpeed, formatEta, joinPath } from "../utils/format";

export const DownloadDetailModal: React.FC = () => {
  const {
    selectedId,
    downloads,
    isDetailModalOpen,
    setDetailModalOpen,
    setChecksumTargetItem,
    activeSegments,
    loadSegments,
    openFolder,
    speedHistory,
    t,
  } = useAppStore();

  const [activeTab, setActiveTab] = useState<"segments" | "speed" | "info">("segments");

  const item = downloads.find((d) => d.id === selectedId);

  useEffect(() => {
    if (isDetailModalOpen && selectedId) {
      loadSegments(selectedId);
      const interval = setInterval(() => {
        loadSegments(selectedId);
      }, 500);
      return () => clearInterval(interval);
    }
  }, [isDetailModalOpen, selectedId, loadSegments]);

  if (!isDetailModalOpen || !item) return null;

  const speeds = speedHistory[item.id] || [];

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
      <div className="w-full max-w-2xl rounded-xl border border-tg-outline/40 bg-tg-card shadow-2xl overflow-hidden text-xs">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-tg-outline/30 px-4 py-3 bg-tg-low">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <span className="rounded bg-tg-primary-container/20 px-2 py-0.5 font-mono text-[10px] font-semibold uppercase text-tg-primary">
                {activeSegments.length || 8} THREADS DUPLEX
              </span>
              <span className="font-mono text-[10px] uppercase text-tg-secondary">
                {item.status}
              </span>
            </div>
            <h2 className="mt-1 font-semibold text-sm text-tg-ink truncate max-w-md">{item.file_name}</h2>
            <p className="text-[11px] text-tg-muted font-mono tabular-nums">
              {formatBytes(item.downloaded)} / {formatBytes(item.total_bytes)} ({item.progress_percent.toFixed(1)}%) • {formatSpeed(item.speed_bps)} • ETA {formatEta(item.eta_seconds)}
            </p>
          </div>
          <button
            onClick={() => setDetailModalOpen(false)}
            className="rounded-md p-1 text-tg-muted hover:bg-tg-high hover:text-tg-ink"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* Tabs */}
        <div className="flex border-b border-tg-outline/30 bg-tg-lowest/70 px-4">
          <button
            onClick={() => setActiveTab("segments")}
            className={`flex items-center gap-1.5 border-b-2 py-2.5 px-3 font-medium transition ${
              activeTab === "segments"
                ? "border-tg-primary text-tg-primary"
                : "border-transparent text-tg-muted hover:text-tg-ink"
            }`}
          >
            <Layers className="h-3.5 w-3.5" />
            {t("tab_segments")}
          </button>
          <button
            onClick={() => setActiveTab("speed")}
            className={`flex items-center gap-1.5 border-b-2 py-2.5 px-3 font-medium transition ${
              activeTab === "speed"
                ? "border-tg-primary text-tg-primary"
                : "border-transparent text-tg-muted hover:text-tg-ink"
            }`}
          >
            <Activity className="h-3.5 w-3.5" />
            {t("tab_speed")}
          </button>
          <button
            onClick={() => setActiveTab("info")}
            className={`flex items-center gap-1.5 border-b-2 py-2.5 px-3 font-medium transition ${
              activeTab === "info"
                ? "border-tg-primary text-tg-primary"
                : "border-transparent text-tg-muted hover:text-tg-ink"
            }`}
          >
            <Info className="h-3.5 w-3.5" />
            {t("tab_info")}
          </button>
        </div>

        {/* Tab Contents */}
        <div className="p-4 min-h-[240px] max-h-[360px] overflow-auto text-tg-ink">
          {activeTab === "segments" && (
            <div className="space-y-3">
              <div className="text-tg-muted text-[11px] flex justify-between">
                <span>Visualisasi Segmen Paralel Aktif</span>
                <span className="font-mono text-tg-secondary font-semibold">
                  {activeSegments.length || 8} Segmen
                </span>
              </div>

              <div className="space-y-2">
                {(activeSegments.length > 0
                  ? activeSegments
                  : Array.from({ length: 8 }).map((_, i) => ({
                      idx: i,
                      start_byte: 0,
                      end_byte: 100,
                      written: item.status === "completed" ? 100 : item.progress_percent,
                    }))
                ).map((seg) => {
                  const segTotal = seg.end_byte - seg.start_byte + 1;
                  const pct = segTotal > 0 ? (seg.written / segTotal) * 100 : 0;
                  return (
                    <div key={seg.idx} className="space-y-1">
                      <div className="flex justify-between text-[10px] text-tg-muted font-mono tabular-nums">
                        <span>Segmen #{seg.idx + 1}</span>
                        <span>{pct.toFixed(1)}%</span>
                      </div>
                      <div className="h-2 w-full overflow-hidden rounded-full bg-tg-lowest">
                        <div
                          className="h-full bg-tg-secondary transition-all duration-300"
                          style={{ width: `${Math.min(100, Math.max(0, pct))}%` }}
                        />
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {activeTab === "speed" && (
            <div className="space-y-3">
              <div className="flex justify-between text-tg-muted text-[11px]">
                <span>Grafik Kecepatan Realtime</span>
                <span className="font-mono text-tg-secondary font-bold">
                  {formatSpeed(item.speed_bps)}
                </span>
              </div>

              <div className="h-40 w-full rounded-lg border border-tg-outline/30 bg-tg-lowest p-3 flex items-end">
                {speeds.length > 0 ? (
                  <svg className="h-full w-full overflow-visible" viewBox="0 0 100 100" preserveAspectRatio="none">
                    {(() => {
                      const maxSpeed = Math.max(...speeds, 1024 * 1024);
                      const points = speeds
                        .map((s, idx) => {
                          const x = (idx / Math.max(speeds.length - 1, 1)) * 100;
                          const y = 100 - (s / maxSpeed) * 85;
                          return `${x},${y}`;
                        })
                        .join(" ");

                      return (
                        <polyline
                          fill="none"
                          stroke="rgb(var(--tg-secondary))"
                          strokeWidth="2.5"
                          points={points}
                        />
                      );
                    })()}
                  </svg>
                ) : (
                  <div className="w-full text-center text-tg-muted text-xs py-12">
                    Belum ada data kecepatan realtime.
                  </div>
                )}
              </div>
            </div>
          )}

          {activeTab === "info" && (
            <div className="space-y-2 font-mono text-[11px] text-tg-ink divide-y divide-tg-outline/20">
              <div className="pt-1.5 flex justify-between gap-4">
                <span className="text-tg-muted">ID Unduhan:</span>
                <span className="truncate max-w-xs">{item.id}</span>
              </div>
              <div className="pt-1.5 flex justify-between gap-4">
                <span className="text-tg-muted">URL Asli:</span>
                <span className="truncate max-w-xs text-tg-primary" title={item.url}>
                  {item.url}
                </span>
              </div>
              <div className="pt-1.5 flex justify-between gap-4">
                <span className="text-tg-muted">Folder Tujuan:</span>
                <span className="truncate max-w-xs">{item.save_dir}</span>
              </div>
              <div className="pt-1.5 flex justify-between gap-4">
                <span className="text-tg-muted">ETag:</span>
                <span>{item.etag || "--"}</span>
              </div>
              <div className="pt-1.5 flex justify-between gap-4">
                <span className="text-tg-muted">Status:</span>
                <span className="uppercase text-tg-secondary">{item.status}</span>
              </div>
              {item.error && (
                <div className="pt-1.5 flex justify-between gap-4">
                  <span className="text-tg-error">Error:</span>
                  <span className="text-tg-error break-all">{item.error}</span>
                </div>
              )}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between border-t border-tg-outline/30 bg-tg-low px-4 py-2.5">
          <div className="flex items-center gap-2">
            {item.status === "completed" && (
              <button
                onClick={() => {
                  setDetailModalOpen(false);
                  setChecksumTargetItem(item);
                }}
                className="flex items-center gap-1.5 rounded-lg border border-tg-secondary/40 bg-tg-secondary/15 px-3 py-1.5 font-medium text-tg-secondary hover:bg-tg-secondary/25"
              >
                <ShieldCheck className="h-3.5 w-3.5" />
                Periksa SHA-256
              </button>
            )}
            <button
              onClick={() => openFolder(joinPath(item.save_dir, item.file_name))}
              className="flex items-center gap-1.5 rounded-lg border border-tg-outline/40 bg-tg-high px-3 py-1.5 font-medium text-tg-ink hover:bg-tg-highest"
            >
              <FolderOpen className="h-3.5 w-3.5 text-tg-tertiary" />
              {t("open_folder")}
            </button>
          </div>
          <button
            onClick={() => setDetailModalOpen(false)}
            className="rounded-lg bg-tg-high px-4 py-1.5 text-tg-ink hover:bg-tg-highest font-medium border border-tg-outline/40"
          >
            {t("btn_close")}
          </button>
        </div>
      </div>
    </div>
  );
};
