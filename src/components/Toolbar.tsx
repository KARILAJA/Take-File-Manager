import React, { useState } from "react";
import {
  Plus,
  Play,
  Pause,
  Square,
  Trash2,
  FolderOpen,
  FileText,
  Sun,
  Moon,
  Globe,
  BarChart2,
  Settings,
  Gauge,
  CalendarClock,
  ShieldCheck,
  Radar,
  Puzzle,
  Minus,
  Maximize2,
  Copy,
  X,
} from "lucide-react";
import { useAppStore, tauriInvoke } from "../store";
import { joinPath } from "../utils/format";
import tfmLogo from "../assets/tfm-logo.png";

export const Toolbar: React.FC = () => {
  const {
    selectedId,
    selectedIds,
    downloads,
    activeView,
    theme,
    lang,
    networkSettings,
    setActiveView,
    toggleTheme,
    setLanguage,
    setAddModalOpen,
    setDetailModalOpen,
    setSettingsModalOpen,
    setDeleteModalTargetIds,
    setChecksumTargetItem,
    setGlobalSpeedLimit,
    resumeDownload,
    pauseDownload,
    pauseAllDownloads,
    openFolder,
    openFile,
    t,
  } = useAppStore();

  const [isMaximized, setIsMaximized] = useState(false);

  const selectedItem = downloads.find((d) => d.id === selectedId);
  const isDownloading = selectedItem?.status === "downloading";
  const isPaused = selectedItem?.status === "paused" || selectedItem?.status === "failed";
  const isCompleted = selectedItem?.status === "completed";
  const activeCount = downloads.filter(
    (d) => d.status === "downloading" || d.status === "probing"
  ).length;
  const deleteIds = selectedIds.length > 0 ? selectedIds : selectedId ? [selectedId] : [];

  const handleResume = () => {
    if (selectedId) resumeDownload(selectedId);
  };

  const handlePause = () => {
    if (selectedId) pauseDownload(selectedId);
  };

  const handleDelete = () => {
    if (deleteIds.length > 0) {
      setDeleteModalTargetIds(deleteIds);
    }
  };

  const handleOpenFolder = () => {
    if (selectedItem) {
      openFolder(joinPath(selectedItem.save_dir, selectedItem.file_name));
    }
  };

  const handleOpenFile = () => {
    if (selectedItem) {
      openFile(joinPath(selectedItem.save_dir, selectedItem.file_name));
    }
  };

  const handleDragMouseDown = async (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    if (e.detail === 2) {
      handleToggleMaximize();
      return;
    }
    if (
      typeof window !== "undefined" &&
      (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__
    ) {
      try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        await getCurrentWindow().startDragging();
      } catch {
        // Fallback relies on data-tauri-drag-region attribute
      }
    }
  };

  const handleMinimize = async () => {
    await tauriInvoke("window_minimize");
  };

  const handleToggleMaximize = async () => {
    const res = await tauriInvoke<boolean>("window_toggle_maximize");
    if (typeof res === "boolean") setIsMaximized(res);
    else setIsMaximized((v) => !v);
  };

  const handleCloseToTray = async () => {
    await tauriInvoke("window_close_to_tray");
  };

  return (
    <header className="flex flex-col border-b border-tg-outline/40 bg-tg-lowest select-none z-20 shadow-[0_1px_8px_rgba(0,0,0,0.25)]">
      {/* Tier 1: Stitch Custom Frameless Title Bar (h-9) */}
      <div
        data-tauri-drag-region
        onMouseDown={handleDragMouseDown}
        className="flex h-9 w-full items-center justify-between border-b border-tg-outline/25 bg-tg-lowest px-2.5 text-[11px] text-tg-muted"
      >
        <div className="flex items-center gap-2.5" data-tauri-drag-region>
          <button
            type="button"
            onClick={() => setActiveView("downloads")}
            className="flex items-center gap-2 text-tg-ink hover:opacity-90 transition-opacity"
          >
            <img
              src={tfmLogo}
              alt="Take File Manager Logo"
              className="h-6 w-auto object-contain rounded"
            />
            <span className="text-[13px] font-bold tracking-tight text-tg-ink">
              {t("app_name")}
            </span>
          </button>
          <span className="hidden sm:inline px-1.5 py-0.5 rounded bg-tg-high text-tg-primary font-mono text-[10px] font-semibold">
            v0.1.0 PRO
          </span>

          <nav className="hidden md:flex items-center gap-1 ml-3">
            <button
              type="button"
              onClick={() => setActiveView("downloads")}
              className={`rounded px-2 py-1 text-[11px] transition-colors ${
                activeView === "downloads"
                  ? "text-tg-primary font-bold bg-tg-high/70"
                  : "text-tg-muted hover:bg-tg-low hover:text-tg-ink"
              }`}
            >
              Tasks
            </button>
            <button
              type="button"
              onClick={() => setAddModalOpen(true)}
              className="rounded px-2 py-1 text-[11px] text-tg-muted hover:bg-tg-low hover:text-tg-ink transition-colors"
            >
              File
            </button>
            <button
              type="button"
              onClick={() => setActiveView("grabber")}
              className={`rounded px-2 py-1 text-[11px] transition-colors ${
                activeView === "grabber"
                  ? "text-tg-primary font-bold bg-tg-high/70"
                  : "text-tg-muted hover:bg-tg-low hover:text-tg-ink"
              }`}
            >
              Grabber
            </button>
            <button
              type="button"
              onClick={() => setActiveView("scheduler")}
              className={`rounded px-2 py-1 text-[11px] transition-colors ${
                activeView === "scheduler"
                  ? "text-tg-primary font-bold bg-tg-high/70"
                  : "text-tg-muted hover:bg-tg-low hover:text-tg-ink"
              }`}
            >
              Scheduler
            </button>
            <button
              type="button"
              onClick={() => setSettingsModalOpen(true, "browsers")}
              className="rounded px-2 py-1 text-[11px] text-tg-muted hover:bg-tg-low hover:text-tg-ink transition-colors"
            >
              Ekstensi Browser
            </button>
            <button
              type="button"
              onClick={() => setSettingsModalOpen(true, "network")}
              className="rounded px-2 py-1 text-[11px] text-tg-muted hover:bg-tg-low hover:text-tg-ink transition-colors"
            >
              {t("options")}
            </button>
          </nav>
        </div>

        {/* Drag Spacer */}
        <div className="flex-1 h-full" data-tauri-drag-region />

        {/* Right Title Bar Controls: Throttle pill, Theme Toggle, Native Window Controls */}
        <div className="flex items-center gap-1">
          {networkSettings.speed_limit_kbps > 0 && (
            <span className="hidden lg:inline-flex items-center gap-1 rounded bg-tg-tertiary/15 px-1.5 py-0.5 text-[10px] font-mono text-tg-tertiary border border-tg-tertiary/30 mr-1">
              Limit: {networkSettings.speed_limit_kbps >= 1024
                ? `${(networkSettings.speed_limit_kbps / 1024).toFixed(1)} MB/s`
                : `${networkSettings.speed_limit_kbps} KB/s`}
            </span>
          )}

          {/* Theme Toggle (Stitch top-right position) */}
          <button
            type="button"
            onClick={toggleTheme}
            title="Toggle Theme"
            className="w-7 h-6 rounded-md bg-tg-card hover:bg-tg-high border border-tg-outline/30 flex items-center justify-center transition-all mr-1.5"
          >
            {theme === "dark" ? (
              <Sun className="h-3.5 w-3.5 text-tg-tertiary" />
            ) : (
              <Moon className="h-3.5 w-3.5 text-tg-primary" />
            )}
          </button>

          {/* Custom Window Controls (Minimize / Maximize / Close-to-tray) */}
          <button
            type="button"
            onClick={handleMinimize}
            title="Minimize"
            aria-label="Minimize"
            className="w-8 h-7 rounded flex items-center justify-center text-tg-muted hover:bg-tg-high hover:text-tg-ink transition-colors"
          >
            <Minus className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            onClick={handleToggleMaximize}
            title={isMaximized ? "Restore" : "Maximize"}
            aria-label="Maximize"
            className="w-8 h-7 rounded flex items-center justify-center text-tg-muted hover:bg-tg-high hover:text-tg-ink transition-colors"
          >
            {isMaximized ? (
              <Copy className="h-3 w-3" />
            ) : (
              <Maximize2 className="h-3 w-3" />
            )}
          </button>
          <button
            type="button"
            onClick={handleCloseToTray}
            title="Tutup ke Tray"
            aria-label="Close"
            className="w-8 h-7 rounded flex items-center justify-center text-tg-muted hover:bg-tg-error hover:text-white transition-colors"
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {/* Tier 2: Stitch Icon Action Toolbar (h-12) */}
      <div className="flex h-12 items-center justify-between bg-tg-low/85 px-3 gap-2">
        {/* Left Action Cluster */}
        <div className="flex items-center gap-1.5 overflow-x-auto">
          <button
            type="button"
            onClick={() => setAddModalOpen(true)}
            className="flex items-center gap-1.5 rounded-md bg-tg-primary-container px-3 py-1.5 text-xs font-semibold text-white shadow-sm hover:brightness-110 active:scale-95 transition-all"
          >
            <Plus className="h-3.5 w-3.5" />
            <span>{t("add_url")}</span>
          </button>

          <div className="h-6 w-px bg-tg-outline/40 mx-1" />

          <button
            type="button"
            disabled={!selectedId || !isPaused}
            onClick={handleResume}
            title={t("resume")}
            className="flex items-center gap-1 rounded-md px-2 py-1.5 text-xs font-medium text-tg-ink/80 hover:bg-tg-high hover:text-tg-secondary disabled:opacity-35 disabled:hover:bg-transparent transition-colors"
          >
            <Play className="h-3.5 w-3.5 text-tg-secondary" />
            <span className="hidden lg:inline">{t("resume")}</span>
          </button>

          <button
            type="button"
            disabled={!selectedId || !isDownloading}
            onClick={handlePause}
            title={t("pause")}
            className="flex items-center gap-1 rounded-md px-2 py-1.5 text-xs font-medium text-tg-ink/80 hover:bg-tg-high hover:text-tg-tertiary disabled:opacity-35 disabled:hover:bg-transparent transition-colors"
          >
            <Pause className="h-3.5 w-3.5 text-tg-tertiary" />
            <span className="hidden lg:inline">{t("pause")}</span>
          </button>

          <button
            type="button"
            disabled={activeCount === 0}
            onClick={() => pauseAllDownloads()}
            title={t("stop_all")}
            className="flex items-center gap-1 rounded-md px-2 py-1.5 text-xs font-medium text-tg-ink/80 hover:bg-tg-high hover:text-tg-error disabled:opacity-35 disabled:hover:bg-transparent transition-colors"
          >
            <Square className="h-3.5 w-3.5 text-tg-error" />
            <span className="hidden xl:inline">{t("stop_all")}</span>
          </button>

          <button
            type="button"
            disabled={deleteIds.length === 0}
            onClick={handleDelete}
            title={t("delete")}
            className="flex items-center gap-1 rounded-md px-2 py-1.5 text-xs font-medium text-tg-ink/80 hover:bg-tg-error/15 hover:text-tg-error disabled:opacity-35 disabled:hover:bg-transparent transition-colors"
          >
            <Trash2 className="h-3.5 w-3.5" />
            <span className="hidden lg:inline">
              {t("delete")}
              {deleteIds.length > 0 ? ` (${deleteIds.length})` : ""}
            </span>
          </button>

          <div className="h-6 w-px bg-tg-outline/40 mx-1" />

          <button
            type="button"
            disabled={!selectedId || !isCompleted}
            onClick={handleOpenFile}
            title={t("open_file")}
            className="rounded-md p-1.5 text-tg-muted hover:bg-tg-high hover:text-tg-primary disabled:opacity-35 disabled:hover:bg-transparent transition-colors"
          >
            <FileText className="h-4 w-4" />
          </button>

          <button
            type="button"
            disabled={!selectedId}
            onClick={handleOpenFolder}
            title={t("open_folder")}
            className="rounded-md p-1.5 text-tg-muted hover:bg-tg-high hover:text-tg-tertiary disabled:opacity-35 disabled:hover:bg-transparent transition-colors"
          >
            <FolderOpen className="h-4 w-4" />
          </button>

          <button
            type="button"
            disabled={!selectedId}
            onClick={() => setDetailModalOpen(true)}
            title="Detail & Segmen"
            className="rounded-md p-1.5 text-tg-muted hover:bg-tg-high hover:text-tg-primary disabled:opacity-35 disabled:hover:bg-transparent transition-colors"
          >
            <BarChart2 className="h-4 w-4" />
          </button>

          <button
            type="button"
            disabled={!selectedId || !isCompleted}
            onClick={() => selectedItem && setChecksumTargetItem(selectedItem)}
            title={t("checksum_title")}
            className="rounded-md p-1.5 text-tg-muted hover:bg-tg-high hover:text-tg-secondary disabled:opacity-35 disabled:hover:bg-transparent transition-colors"
          >
            <ShieldCheck className="h-4 w-4" />
          </button>

          <div className="h-6 w-px bg-tg-outline/40 mx-1" />

          <button
            type="button"
            onClick={() => setActiveView(activeView === "scheduler" ? "downloads" : "scheduler")}
            title={t("scheduler")}
            className={`flex items-center gap-1 rounded-md px-2 py-1.5 text-xs font-medium transition-colors ${
              activeView === "scheduler"
                ? "bg-tg-primary-container/20 text-tg-primary border border-tg-primary/30"
                : "text-tg-muted hover:bg-tg-high hover:text-tg-ink"
            }`}
          >
            <CalendarClock className="h-3.5 w-3.5" />
            <span className="hidden xl:inline">{t("scheduler")}</span>
          </button>

          <button
            type="button"
            onClick={() => setActiveView(activeView === "grabber" ? "downloads" : "grabber")}
            title={t("grabber")}
            className={`flex items-center gap-1 rounded-md px-2 py-1.5 text-xs font-medium transition-colors ${
              activeView === "grabber"
                ? "bg-tg-primary-container/20 text-tg-primary border border-tg-primary/30"
                : "text-tg-muted hover:bg-tg-high hover:text-tg-ink"
            }`}
          >
            <Radar className="h-3.5 w-3.5" />
            <span className="hidden xl:inline">{t("grabber")}</span>
          </button>

          <button
            type="button"
            onClick={() => setSettingsModalOpen(true, "browsers")}
            title="Integrasi Ekstensi Browser (Chrome, Edge, Opera, Firefox)"
            className="flex items-center gap-1 rounded-md px-2 py-1.5 text-xs font-medium text-tg-muted hover:bg-tg-high hover:text-tg-primary transition-colors"
          >
            <Puzzle className="h-3.5 w-3.5 text-tg-secondary" />
            <span className="hidden xl:inline">Ekstensi</span>
          </button>
        </div>

        {/* Right Controls: Speed Limiter, Language & Settings */}
        <div className="flex items-center gap-1.5 text-xs flex-shrink-0">
          <div
            className="flex items-center gap-1 rounded-md border border-tg-outline/40 bg-tg-card px-2 py-1 text-tg-ink"
            title="Limiter Kecepatan Unduhan Global"
          >
            <Gauge className="h-3.5 w-3.5 text-tg-secondary" />
            <select
              aria-label="Limiter Kecepatan"
              value={networkSettings.speed_limit_kbps || 0}
              onChange={(e) => setGlobalSpeedLimit(Number(e.target.value))}
              className="bg-transparent text-xs font-medium text-tg-ink focus:outline-none cursor-pointer"
            >
              <option value={0} className="bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100">
                Limit: Tanpa Batas
              </option>
              <option value={256} className="bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100">
                Limit: 256 KB/s
              </option>
              <option value={512} className="bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100">
                Limit: 512 KB/s
              </option>
              <option value={1024} className="bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100">
                Limit: 1 MB/s
              </option>
              <option value={2048} className="bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100">
                Limit: 2 MB/s
              </option>
              <option value={5120} className="bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100">
                Limit: 5 MB/s
              </option>
            </select>
          </div>

          {/* Language Selector */}
          <button
            type="button"
            onClick={() => setLanguage(lang === "id" ? "en" : "id")}
            className="flex items-center gap-1 rounded-md border border-tg-outline/40 bg-tg-card px-2 py-1 text-tg-ink hover:bg-tg-high transition"
            title="Ganti Bahasa / Switch Language"
          >
            <Globe className="h-3.5 w-3.5 text-tg-muted" />
            <span className="font-semibold uppercase">{lang}</span>
          </button>

          {/* Settings & Log Viewer */}
          <button
            type="button"
            onClick={() => setSettingsModalOpen(true, "network")}
            className="rounded-md border border-tg-outline/40 bg-tg-card p-1.5 text-tg-ink hover:bg-tg-high transition"
            title="Pengaturan & Log"
          >
            <Settings className="h-3.5 w-3.5 text-tg-muted" />
          </button>
        </div>
      </div>
    </header>
  );
};
