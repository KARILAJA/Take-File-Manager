import React from "react";
import {
  Download,
  CheckCircle,
  PauseCircle,
  AlertCircle,
  RefreshCw,
  Search,
  ArrowUpDown,
  Play,
  Pause,
  Trash2,
  FolderOpen,
  BarChart2,
  ShieldCheck,
  LayoutList,
  LayoutGrid,
  Activity,
  ChevronDown,
  ChevronUp,
  CheckSquare,
  Square,
} from "lucide-react";
import { useAppStore, DownloadItem } from "../store";
import { formatBytes, formatSpeed, formatEta, formatDate, joinPath } from "../utils/format";

export const DownloadList: React.FC = () => {
  const {
    downloads,
    selectedId,
    selectedIds,
    viewMode,
    telemetryExpanded,
    telemetryMode,
    filter,
    searchQuery,
    sortField,
    sortAsc,
    globalSpeedHistory,
    setSelectedId,
    toggleSelectId,
    setSelectedIds,
    clearSelectedIds,
    setViewMode,
    setTelemetryExpanded,
    setTelemetryMode,
    setFilter,
    setSearchQuery,
    setSort,
    setDetailModalOpen,
    setDeleteModalTargetIds,
    setChecksumTargetItem,
    resumeDownload,
    pauseDownload,
    bulkResume,
    bulkPause,
    openFile,
    openFolder,
    t,
  } = useAppStore();

  // Filter items
  const filteredDownloads = downloads.filter((item) => {
    if (filter === "downloading" && item.status !== "downloading" && item.status !== "probing") {
      return false;
    }
    if (filter === "unfinished" && item.status === "completed") {
      return false;
    }
    if (filter === "completed" && item.status !== "completed") {
      return false;
    }
    if (
      filter === "paused" &&
      item.status !== "paused" &&
      item.status !== "failed" &&
      item.status !== "waiting_refresh"
    ) {
      return false;
    }
    if (
      ["compressed", "documents", "music", "programs", "video", "images", "torrent", "others"].includes(
        filter
      ) &&
      item.category !== filter
    ) {
      return false;
    }

    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      const matchName = item.file_name.toLowerCase().includes(q);
      const matchUrl = item.url.toLowerCase().includes(q);
      if (!matchName && !matchUrl) return false;
    }

    return true;
  });

  // Sort items
  const sortedDownloads = [...filteredDownloads].sort((a, b) => {
    let aVal = a[sortField];
    let bVal = b[sortField];
    if (aVal === undefined || aVal === null) return 1;
    if (bVal === undefined || bVal === null) return -1;
    if (typeof aVal === "string") {
      aVal = aVal.toLowerCase();
      bVal = (bVal as string).toLowerCase();
    }
    if (aVal < bVal) return sortAsc ? -1 : 1;
    if (aVal > bVal) return sortAsc ? 1 : -1;
    return 0;
  });

  const allVisibleSelected =
    sortedDownloads.length > 0 &&
    sortedDownloads.every((d) => selectedIds.includes(d.id));

  const toggleSelectAllVisible = () => {
    if (allVisibleSelected) {
      clearSelectedIds();
    } else {
      setSelectedIds(sortedDownloads.map((d) => d.id));
    }
  };

  const getStatusBadge = (status: string) => {
    switch (status) {
      case "downloading":
        return (
          <span className="inline-flex items-center gap-1 rounded-full bg-tg-primary-container/15 px-2 py-0.5 text-[11px] font-medium text-tg-primary border border-tg-primary/30 animate-pulse">
            <Download className="h-3 w-3" />
            Mengunduh
          </span>
        );
      case "probing":
        return (
          <span className="inline-flex items-center gap-1 rounded-full bg-tg-tertiary/15 px-2 py-0.5 text-[11px] font-medium text-tg-tertiary border border-tg-tertiary/30">
            <RefreshCw className="h-3 w-3 animate-spin" />
            Probing...
          </span>
        );
      case "completed":
        return (
          <span className="inline-flex items-center gap-1 rounded-full bg-tg-secondary/15 px-2 py-0.5 text-[11px] font-medium text-tg-secondary border border-tg-secondary/30">
            <CheckCircle className="h-3 w-3" />
            Selesai
          </span>
        );
      case "paused":
        return (
          <span className="inline-flex items-center gap-1 rounded-full bg-tg-high px-2 py-0.5 text-[11px] font-medium text-tg-muted border border-tg-outline/40">
            <PauseCircle className="h-3 w-3" />
            Dijeda
          </span>
        );
      case "waiting_refresh":
        return (
          <span className="inline-flex items-center gap-1 rounded-full bg-tg-tertiary/15 px-2 py-0.5 text-[11px] font-medium text-tg-tertiary border border-tg-tertiary/30">
            <AlertCircle className="h-3 w-3" />
            Perlu Link Baru
          </span>
        );
      case "failed":
        return (
          <span className="inline-flex items-center gap-1 rounded-full bg-tg-error/15 px-2 py-0.5 text-[11px] font-medium text-tg-error border border-tg-error/30">
            <AlertCircle className="h-3 w-3" />
            Gagal
          </span>
        );
      default:
        return (
          <span className="inline-flex items-center rounded-full bg-tg-high px-2 py-0.5 text-[11px] text-tg-muted">
            {status}
          </span>
        );
    }
  };

  const getHostLabel = (url: string) => {
    if (url.toLowerCase().startsWith("magnet:")) return "BitTorrent Magnet";
    try {
      return new URL(url).hostname;
    } catch {
      return "Local / Direct";
    }
  };

  const handleDoubleClick = (item: DownloadItem) => {
    if (item.status === "completed") {
      openFile(joinPath(item.save_dir, item.file_name));
    } else {
      setSelectedId(item.id);
      setDetailModalOpen(true);
    }
  };

  // Telemetry KPI aggregates
  const activeDownloads = downloads.filter(
    (d) => d.status === "downloading" || d.status === "probing"
  );
  const completedCount = downloads.filter((d) => d.status === "completed").length;
  const totalSpeedBps = activeDownloads.reduce((sum, d) => sum + (d.speed_bps || 0), 0);
  const sparkPoints = globalSpeedHistory.length > 0 ? globalSpeedHistory : [0, 0, 0, 0];
  const maxSpark = Math.max(...sparkPoints, 1024 * 1024);

  return (
    <div className="flex flex-1 flex-col overflow-hidden bg-tg-bg">
      {/* Sub-header: Filter Pills, Search, Sort & View Mode */}
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-tg-outline/30 bg-tg-low/50 px-3 py-2 text-xs">
        <div className="flex items-center gap-2">
          {/* Quick Status Pills */}
          <div className="hidden xl:flex items-center gap-1 rounded-md bg-tg-lowest p-0.5 border border-tg-outline/30">
            {[
              { id: "all", label: "All" },
              { id: "downloading", label: "Active" },
              { id: "completed", label: "Completed" },
              { id: "paused", label: "Paused" },
            ].map((pill) => (
              <button
                key={pill.id}
                onClick={() => setFilter(pill.id)}
                className={`rounded px-2 py-1 text-[11px] font-medium transition-colors ${
                  filter === pill.id
                    ? "bg-tg-primary-container text-white"
                    : "text-tg-muted hover:text-tg-ink"
                }`}
              >
                {pill.label}
              </button>
            ))}
          </div>

          <div className="relative flex items-center w-64 sm:w-72">
            <Search className="absolute left-2.5 h-3.5 w-3.5 text-tg-muted" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder={t("search_placeholder")}
              className="w-full rounded-md bg-tg-lowest border border-tg-outline/40 py-1 pl-8 pr-3 text-xs text-tg-ink placeholder-tg-muted/60 focus:border-tg-primary focus:outline-none focus:ring-1 focus:ring-tg-primary"
            />
          </div>
        </div>

        <div className="flex items-center gap-2">
          <button
            onClick={toggleSelectAllVisible}
            className="flex items-center gap-1 rounded-md border border-tg-outline/40 bg-tg-card px-2 py-1 text-[11px] font-medium text-tg-ink hover:bg-tg-high transition"
          >
            {allVisibleSelected ? (
              <CheckSquare className="h-3.5 w-3.5 text-tg-primary" />
            ) : (
              <Square className="h-3.5 w-3.5 text-tg-muted" />
            )}
            <span>{t("bulk_select_all")}</span>
          </button>

          <div className="flex items-center rounded-md border border-tg-outline/40 bg-tg-lowest p-0.5">
            <button
              onClick={() => setViewMode("table")}
              title={t("view_table")}
              className={`rounded p-1 transition ${
                viewMode === "table"
                  ? "bg-tg-primary-container text-white"
                  : "text-tg-muted hover:text-tg-ink"
              }`}
            >
              <LayoutList className="h-3.5 w-3.5" />
            </button>
            <button
              onClick={() => setViewMode("cards")}
              title={t("view_cards")}
              className={`rounded p-1 transition ${
                viewMode === "cards"
                  ? "bg-tg-primary-container text-white"
                  : "text-tg-muted hover:text-tg-ink"
              }`}
            >
              <LayoutGrid className="h-3.5 w-3.5" />
            </button>
          </div>

          <div className="text-[11px] text-tg-muted hidden sm:block">
            Menampilkan <span className="text-tg-ink font-semibold tabular-nums">{sortedDownloads.length}</span> unduhan
          </div>
        </div>
      </div>

      {/* Bulk Selection Action Bar (shown when >=1 item is checked) */}
      {selectedIds.length > 0 && (
        <div className="flex items-center justify-between border-b border-tg-primary/30 bg-tg-primary-container/15 px-3 py-1.5 text-xs">
          <div className="flex items-center gap-2 font-semibold text-tg-primary">
            <CheckSquare className="h-3.5 w-3.5" />
            <span>{selectedIds.length} terpilih</span>
            <button
              onClick={clearSelectedIds}
              className="text-[11px] font-normal text-tg-muted hover:text-tg-ink underline ml-1"
            >
              {t("bulk_clear")}
            </button>
          </div>
          <div className="flex items-center gap-1.5">
            <button
              onClick={() => bulkResume(selectedIds)}
              className="flex items-center gap-1 rounded bg-tg-secondary/20 border border-tg-secondary/40 px-2.5 py-1 text-[11px] font-semibold text-tg-secondary hover:bg-tg-secondary/30"
            >
              <Play className="h-3 w-3" />
              {t("bulk_resume")}
            </button>
            <button
              onClick={() => bulkPause(selectedIds)}
              className="flex items-center gap-1 rounded bg-tg-tertiary/20 border border-tg-tertiary/40 px-2.5 py-1 text-[11px] font-semibold text-tg-tertiary hover:bg-tg-tertiary/30"
            >
              <Pause className="h-3 w-3" />
              {t("bulk_pause")}
            </button>
            <button
              onClick={() => setDeleteModalTargetIds(selectedIds)}
              className="flex items-center gap-1 rounded bg-tg-error/20 border border-tg-error/40 px-2.5 py-1 text-[11px] font-semibold text-tg-error hover:bg-tg-error/30"
            >
              <Trash2 className="h-3 w-3" />
              {t("bulk_delete")}
            </button>
          </div>
        </div>
      )}

      {/* Main Content Container: Table or Cards */}
      <div className="flex-1 overflow-auto">
        {sortedDownloads.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center p-8 text-center text-tg-muted">
            <div className="mb-3 flex h-14 w-14 items-center justify-center rounded-2xl bg-tg-card border border-tg-outline/30">
              <Download className="h-7 w-7 text-tg-primary/60" />
            </div>
            <p className="text-sm font-semibold text-tg-ink">{t("empty_title")}</p>
            <p className="text-xs text-tg-muted max-w-sm mt-1">{t("empty_subtitle")}</p>
          </div>
        ) : viewMode === "cards" ? (
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3 p-3">
            {sortedDownloads.map((item, idx) => {
              const isSelected = selectedId === item.id;
              const isChecked = selectedIds.includes(item.id);
              return (
                <div
                  key={item.id}
                  onClick={() => setSelectedId(item.id)}
                  onDoubleClick={() => handleDoubleClick(item)}
                  className={`rounded-lg border p-3 transition-all cursor-pointer ${
                    isSelected
                      ? "border-tg-primary bg-tg-high/90 shadow-md"
                      : "border-tg-outline/30 bg-tg-card hover:bg-tg-high/50"
                  }`}
                >
                  <div className="flex items-start justify-between gap-2 mb-2">
                    <div className="flex items-center gap-2 min-w-0">
                      <input
                        type="checkbox"
                        checked={isChecked}
                        onChange={(e) => {
                          e.stopPropagation();
                          toggleSelectId(item.id);
                        }}
                        className="rounded border-tg-outline accent-tg-primary-container"
                      />
                      <span className="font-mono text-[10px] text-tg-muted">#{idx + 1}</span>
                      <span className="font-semibold text-tg-ink truncate" title={item.file_name}>
                        {item.file_name}
                      </span>
                    </div>
                    {getStatusBadge(item.status)}
                  </div>
                  <div className="flex justify-between text-[11px] text-tg-muted font-mono mb-1.5 tabular-nums">
                    <span>{formatBytes(item.downloaded)} / {formatBytes(item.total_bytes)}</span>
                    <span>{item.progress_percent.toFixed(1)}%</span>
                  </div>
                  <div className="h-2 w-full overflow-hidden rounded-full bg-tg-lowest mb-2">
                    <div
                      className={`h-full transition-all duration-300 ${
                        item.status === "completed"
                          ? "bg-tg-secondary"
                          : item.status === "failed"
                          ? "bg-tg-error"
                          : "bg-tg-primary-container"
                      }`}
                      style={{ width: `${Math.min(100, Math.max(0, item.progress_percent))}%` }}
                    />
                  </div>
                  <div className="flex items-center justify-between text-[10px] text-tg-muted font-mono">
                    <span>{formatSpeed(item.speed_bps)} • ETA {formatEta(item.eta_seconds)}</span>
                    <div className="flex items-center gap-1" onClick={(e) => e.stopPropagation()}>
                      {item.status === "downloading" ? (
                        <button onClick={() => pauseDownload(item.id)} className="p-1 hover:text-tg-tertiary" title={t("pause")}>
                          <Pause className="h-3.5 w-3.5" />
                        </button>
                      ) : item.status !== "completed" ? (
                        <button onClick={() => resumeDownload(item.id)} className="p-1 hover:text-tg-secondary" title={t("resume")}>
                          <Play className="h-3.5 w-3.5" />
                        </button>
                      ) : (
                        <button onClick={() => setChecksumTargetItem(item)} className="p-1 hover:text-tg-secondary" title={t("checksum_title")}>
                          <ShieldCheck className="h-3.5 w-3.5" />
                        </button>
                      )}
                      <button
                        onClick={() => openFolder(joinPath(item.save_dir, item.file_name))}
                        className="p-1 hover:text-tg-tertiary"
                        title={t("open_folder")}
                      >
                        <FolderOpen className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        ) : (
          <table className="w-full border-collapse text-left text-xs">
            <thead className="sticky top-0 z-10 border-b border-tg-outline/30 bg-tg-low/95 text-tg-muted backdrop-blur">
              <tr>
                <th className="pl-3 pr-1 py-2 w-8 text-center">
                  <input
                    type="checkbox"
                    aria-label={t("bulk_select_all")}
                    checked={allVisibleSelected}
                    onChange={toggleSelectAllVisible}
                    className="rounded border-tg-outline accent-tg-primary-container cursor-pointer"
                  />
                </th>
                <th className="px-2 py-2 font-medium w-10 text-center">{t("col_no")}</th>
                <th
                  onClick={() => setSort("file_name")}
                  className="cursor-pointer px-3 py-2 font-medium hover:text-tg-ink"
                >
                  <div className="flex items-center gap-1">
                    <span>{t("col_name")}</span>
                    <ArrowUpDown className="h-3 w-3 opacity-60" />
                  </div>
                </th>
                <th
                  onClick={() => setSort("total_bytes")}
                  className="cursor-pointer px-3 py-2 font-medium hover:text-tg-ink w-24"
                >
                  <div className="flex items-center gap-1">
                    <span>{t("col_size")}</span>
                    <ArrowUpDown className="h-3 w-3 opacity-60" />
                  </div>
                </th>
                <th className="px-3 py-2 font-medium w-32">{t("col_status")}</th>
                <th
                  onClick={() => setSort("progress_percent")}
                  className="cursor-pointer px-3 py-2 font-medium hover:text-tg-ink w-48"
                >
                  <div className="flex items-center gap-1">
                    <span>{t("col_progress")}</span>
                    <ArrowUpDown className="h-3 w-3 opacity-60" />
                  </div>
                </th>
                <th className="px-3 py-2 font-medium w-24">{t("col_speed")}</th>
                <th className="px-3 py-2 font-medium w-20">{t("col_eta")}</th>
                <th
                  onClick={() => setSort("created_at")}
                  className="cursor-pointer px-3 py-2 font-medium hover:text-tg-ink w-28 hidden lg:table-cell"
                >
                  <div className="flex items-center gap-1">
                    <span>{t("col_date")}</span>
                    <ArrowUpDown className="h-3 w-3 opacity-60" />
                  </div>
                </th>
                <th className="px-3 py-2 font-medium w-24 text-right">{t("col_controls")}</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-tg-outline/20">
              {sortedDownloads.map((item, idx) => {
                const isSelected = selectedId === item.id;
                const isChecked = selectedIds.includes(item.id);
                return (
                  <tr
                    key={item.id}
                    onClick={() => setSelectedId(item.id)}
                    onDoubleClick={() => handleDoubleClick(item)}
                    className={`group cursor-pointer transition-colors ${
                      isSelected
                        ? "bg-tg-high/90 text-tg-ink"
                        : "text-tg-ink/90 hover:bg-tg-card/70"
                    }`}
                  >
                    <td
                      className="pl-3 pr-1 py-2 text-center"
                      onClick={(e) => {
                        e.stopPropagation();
                        toggleSelectId(item.id);
                      }}
                    >
                      <input
                        type="checkbox"
                        checked={isChecked}
                        onChange={() => {}}
                        className="rounded border-tg-outline accent-tg-primary-container cursor-pointer"
                      />
                    </td>
                    <td className="px-2 py-2 text-center font-mono text-tg-muted tabular-nums">
                      {idx + 1}
                    </td>
                    <td className="px-3 py-2 max-w-[260px]" title={item.file_name}>
                      <div className="font-medium truncate">{item.file_name}</div>
                      <div className="flex items-center gap-1.5 text-[10px] text-tg-muted truncate">
                        <span className="uppercase font-mono text-tg-primary/80">{item.category}</span>
                        <span>•</span>
                        <span className="truncate">{getHostLabel(item.url)}</span>
                      </div>
                    </td>
                    <td className="px-3 py-2 font-mono text-tg-muted tabular-nums">
                      {formatBytes(item.total_bytes || item.downloaded)}
                    </td>
                    <td className="px-3 py-2">{getStatusBadge(item.status)}</td>
                    <td className="px-3 py-2">
                      <div className="w-full space-y-1">
                        <div className="flex justify-between text-[10px] text-tg-muted font-mono tabular-nums">
                          <span>{item.progress_percent.toFixed(1)}%</span>
                          <span>
                            {formatBytes(item.downloaded)} / {formatBytes(item.total_bytes)}
                          </span>
                        </div>
                        <div className="h-1.5 w-full overflow-hidden rounded-full bg-tg-lowest">
                          <div
                            className={`h-full transition-all duration-300 ${
                              item.status === "completed"
                                ? "bg-tg-secondary"
                                : item.status === "failed"
                                ? "bg-tg-error"
                                : item.status === "paused"
                                ? "bg-tg-muted/50"
                                : "bg-tg-primary-container"
                            }`}
                            style={{ width: `${Math.min(100, Math.max(0, item.progress_percent))}%` }}
                          />
                        </div>
                      </div>
                    </td>
                    <td className="px-3 py-2 font-mono text-tg-secondary font-medium tabular-nums">
                      {formatSpeed(item.speed_bps)}
                    </td>
                    <td className="px-3 py-2 font-mono text-tg-muted tabular-nums">
                      {formatEta(item.eta_seconds)}
                    </td>
                    <td className="px-3 py-2 text-[11px] text-tg-muted hidden lg:table-cell">
                      {formatDate(item.created_at)}
                    </td>
                    <td
                      className="px-3 py-2 text-right"
                      onClick={(e) => e.stopPropagation()}
                    >
                      <div className="inline-flex items-center justify-end gap-0.5 opacity-75 group-hover:opacity-100">
                        {item.status === "downloading" ? (
                          <button
                            onClick={() => pauseDownload(item.id)}
                            title={t("pause")}
                            className="rounded p-1 hover:bg-tg-lowest text-tg-tertiary"
                          >
                            <Pause className="h-3.5 w-3.5" />
                          </button>
                        ) : item.status !== "completed" ? (
                          <button
                            onClick={() => resumeDownload(item.id)}
                            title={t("resume")}
                            className="rounded p-1 hover:bg-tg-lowest text-tg-secondary"
                          >
                            <Play className="h-3.5 w-3.5" />
                          </button>
                        ) : (
                          <button
                            onClick={() => setChecksumTargetItem(item)}
                            title={t("checksum_title")}
                            className="rounded p-1 hover:bg-tg-lowest text-tg-secondary"
                          >
                            <ShieldCheck className="h-3.5 w-3.5" />
                          </button>
                        )}
                        <button
                          onClick={() => {
                            setSelectedId(item.id);
                            setDetailModalOpen(true);
                          }}
                          title="Detail & Segmen"
                          className="rounded p-1 hover:bg-tg-lowest text-tg-primary"
                        >
                          <BarChart2 className="h-3.5 w-3.5" />
                        </button>
                        <button
                          onClick={() => setDeleteModalTargetIds([item.id])}
                          title={t("delete")}
                          className="rounded p-1 hover:bg-tg-lowest text-tg-error"
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </button>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      {/* Stitch Bottom Docked Panel: Real-Time Ingress Telemetry */}
      <div className="border-t border-tg-outline/30 bg-tg-low/70 text-xs">
        <div className="flex items-center justify-between px-3 py-1.5">
          <button
            onClick={() => setTelemetryExpanded(!telemetryExpanded)}
            className="flex items-center gap-1.5 text-[11px] font-semibold text-tg-ink hover:text-tg-primary transition"
          >
            <Activity className="h-3.5 w-3.5 text-tg-secondary" />
            <span>{t("telemetry_title")}</span>
            {telemetryExpanded ? (
              <ChevronDown className="h-3.5 w-3.5 text-tg-muted" />
            ) : (
              <ChevronUp className="h-3.5 w-3.5 text-tg-muted" />
            )}
          </button>

          {telemetryExpanded && (
            <div className="flex items-center gap-1 rounded bg-tg-lowest p-0.5 border border-tg-outline/30 text-[10px]">
              <button
                onClick={() => setTelemetryMode("area")}
                className={`rounded px-2 py-0.5 font-medium ${
                  telemetryMode === "area" ? "bg-tg-primary-container text-white" : "text-tg-muted"
                }`}
              >
                Area Curve
              </button>
              <button
                onClick={() => setTelemetryMode("bars")}
                className={`rounded px-2 py-0.5 font-medium ${
                  telemetryMode === "bars" ? "bg-tg-primary-container text-white" : "text-tg-muted"
                }`}
              >
                Per File
              </button>
            </div>
          )}
        </div>

        {telemetryExpanded && (
          <div className="grid grid-cols-1 md:grid-cols-12 gap-2 px-3 pb-2.5">
            {/* Sparkline / Per-File Bars (7 cols) */}
            <div className="md:col-span-7 h-16 rounded-md border border-tg-outline/25 bg-tg-lowest/90 p-2 flex items-end overflow-hidden">
              {telemetryMode === "area" ? (
                <svg className="h-full w-full overflow-visible" viewBox="0 0 400 50" preserveAspectRatio="none">
                  <polyline
                    fill="none"
                    stroke="rgb(var(--tg-secondary))"
                    strokeWidth="2"
                    points={sparkPoints
                      .map((val, i) => {
                        const x = (i / Math.max(sparkPoints.length - 1, 1)) * 400;
                        const y = 46 - (val / maxSpark) * 42;
                        return `${x},${y}`;
                      })
                      .join(" ")}
                  />
                </svg>
              ) : activeDownloads.length === 0 ? (
                <div className="w-full text-center text-[10px] text-tg-muted self-center font-mono">
                  Tidak ada transfer aktif saat ini
                </div>
              ) : (
                <div className="w-full flex items-end gap-1.5 h-full">
                  {activeDownloads.slice(0, 12).map((d) => {
                    const hPct = Math.max(12, Math.min(100, d.progress_percent));
                    return (
                      <div key={d.id} className="flex-1 flex flex-col items-center justify-end h-full" title={`${d.file_name}: ${formatSpeed(d.speed_bps)}`}>
                        <div
                          className="w-full rounded-t bg-tg-primary-container/80"
                          style={{ height: `${hPct}%` }}
                        />
                      </div>
                    );
                  })}
                </div>
              )}
            </div>

            {/* Bento KPI Cards (5 cols) */}
            <div className="md:col-span-5 grid grid-cols-3 gap-2">
              <div className="rounded-md border border-tg-outline/25 bg-tg-lowest/90 px-2.5 py-1.5">
                <div className="text-[10px] text-tg-muted">Ingress Speed</div>
                <div className="font-mono text-xs font-bold text-tg-secondary tabular-nums mt-0.5">
                  {formatSpeed(totalSpeedBps)}
                </div>
              </div>
              <div className="rounded-md border border-tg-outline/25 bg-tg-lowest/90 px-2.5 py-1.5">
                <div className="text-[10px] text-tg-muted">Active Tasks</div>
                <div className="font-mono text-xs font-bold text-tg-primary tabular-nums mt-0.5">
                  {activeDownloads.length} / {downloads.length}
                </div>
              </div>
              <div className="rounded-md border border-tg-outline/25 bg-tg-lowest/90 px-2.5 py-1.5">
                <div className="text-[10px] text-tg-muted">Completed</div>
                <div className="font-mono text-xs font-bold text-tg-ink tabular-nums mt-0.5">
                  {completedCount}
                </div>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
