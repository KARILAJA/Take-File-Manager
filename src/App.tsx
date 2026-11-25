import { useEffect } from "react";
import { Toolbar } from "./components/Toolbar";
import { Sidebar } from "./components/Sidebar";
import { DownloadList } from "./components/DownloadList";
import { BatchGrabberView } from "./components/BatchGrabberView";
import { SchedulerQueuesView } from "./components/SchedulerQueuesView";
import { AddUrlModal } from "./components/AddUrlModal";
import { DownloadDetailModal } from "./components/DownloadDetailModal";
import { DownloadCompletedModal } from "./components/DownloadCompletedModal";
import { SettingsLogsModal } from "./components/SettingsLogsModal";
import { DeleteConfirmModal } from "./components/DeleteConfirmModal";
import { ChecksumModal } from "./components/ChecksumModal";
import { ToastOverlay } from "./components/ToastOverlay";
import { useAppStore } from "./store";
import { formatSpeed } from "./utils/format";

export function App() {
  const {
    theme,
    activeView,
    downloads,
    loadDownloads,
    loadQueuesAndSchedules,
    popPendingDownloadPrompt,
    setPendingPrompt,
    setAddModalOpen,
  } = useAppStore();

  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
    document.documentElement.style.colorScheme = theme;
  }, [theme]);

  useEffect(() => {
    loadDownloads();
    loadQueuesAndSchedules();
    useAppStore.getState().loadSettingsAndLogs();

    let pollingPrompt = false;
    const interval = setInterval(async () => {
      loadDownloads();
      if (!pollingPrompt && !useAppStore.getState().isAddModalOpen) {
        pollingPrompt = true;
        try {
          const prompt = await popPendingDownloadPrompt();
          if (prompt && !useAppStore.getState().isAddModalOpen) {
            setPendingPrompt(prompt);
            setAddModalOpen(true);
          }
        } finally {
          pollingPrompt = false;
        }
      }
    }, 500);

    return () => clearInterval(interval);
  }, [
    loadDownloads,
    loadQueuesAndSchedules,
    popPendingDownloadPrompt,
    setPendingPrompt,
    setAddModalOpen,
  ]);

  const activeDownloads = downloads.filter(
    (d) => d.status === "downloading" || d.status === "probing"
  );
  const totalSpeed = activeDownloads.reduce((sum, d) => sum + (d.speed_bps || 0), 0);

  return (
    <div
      className={`flex h-screen w-screen flex-col font-sans transition-colors duration-200 select-none bg-tg-bg text-tg-ink ${
        theme === "dark" ? "dark" : ""
      }`}
    >
      {/* Top 2-Tier Stitch Header & Action Toolbar */}
      <Toolbar />

      {/* Main Body (Sidebar + Active Workspace) */}
      <div className="flex flex-1 overflow-hidden">
        <Sidebar />
        <main className="flex-1 flex flex-col overflow-hidden bg-tg-bg">
          {activeView === "grabber" ? (
            <BatchGrabberView />
          ) : activeView === "scheduler" ? (
            <SchedulerQueuesView />
          ) : (
            <DownloadList />
          )}
        </main>
      </div>

      {/* Stitch Bottom Status Bar (h-7) */}
      <footer className="flex h-7 items-center justify-between border-t border-tg-outline/30 bg-tg-lowest px-4 text-[11px] text-tg-muted select-none">
        <div className="flex items-center gap-4">
          <span className="flex items-center gap-1.5">
            <span
              className={`h-2 w-2 rounded-full ${
                activeDownloads.length > 0 ? "bg-tg-secondary animate-pulse" : "bg-tg-muted/60"
              }`}
            />
            {activeDownloads.length > 0
              ? `${activeDownloads.length} aktif`
              : "Siap (Tauri 2 Engine Online)"}
          </span>
          <span className="font-mono tabular-nums">Total: {downloads.length} unduhan</span>
        </div>
        <div className="flex items-center gap-4 font-mono text-[11px] tabular-nums">
          <span>Kecepatan Total: {formatSpeed(totalSpeed)}</span>
          <span className="text-tg-primary font-semibold">v0.1.0</span>
        </div>
      </footer>

      {/* Interactive Stitch Modals & Toasts */}
      <AddUrlModal />
      <DownloadDetailModal />
      <DownloadCompletedModal />
      <SettingsLogsModal />
      <DeleteConfirmModal />
      <ChecksumModal />
      <ToastOverlay />
    </div>
  );
}

export default App;
