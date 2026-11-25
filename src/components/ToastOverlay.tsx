import React from "react";
import { CheckCircle2, AlertTriangle, FolderOpen, FileText, RotateCcw, X } from "lucide-react";
import { useAppStore } from "../store";
import { formatBytes, joinPath } from "../utils/format";

export const ToastOverlay: React.FC = () => {
  const { toasts, dismissToast, openFile, openFolder, resumeDownload } = useAppStore();

  if (toasts.length === 0) return null;

  return (
    <div className="fixed bottom-10 right-4 z-40 flex flex-col gap-2 w-80 pointer-events-none">
      {toasts.map((toast) => {
        const isCompleted = toast.kind === "completed";
        const fullPath = joinPath(toast.saveDir, toast.fileName);
        return (
          <div
            key={toast.id}
            className={`pointer-events-auto rounded-xl border bg-tg-card/95 backdrop-blur-md p-3 shadow-2xl text-xs transition-all ${
              isCompleted ? "border-tg-secondary/50" : "border-tg-error/50"
            }`}
          >
            <div className="flex items-start justify-between gap-2">
              <div className="flex items-center gap-2">
                {isCompleted ? (
                  <CheckCircle2 className="h-4 w-4 text-tg-secondary flex-shrink-0" />
                ) : (
                  <AlertTriangle className="h-4 w-4 text-tg-error flex-shrink-0" />
                )}
                <span className={`font-semibold ${isCompleted ? "text-tg-secondary" : "text-tg-error"}`}>
                  {isCompleted ? "Unduhan Selesai" : "Unduhan Terputus / Gagal"}
                </span>
              </div>
              <button
                onClick={() => dismissToast(toast.id)}
                className="rounded p-0.5 text-tg-muted hover:bg-tg-high hover:text-tg-ink"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            </div>

            <div className="mt-1.5 font-medium text-tg-ink truncate" title={toast.fileName}>
              {toast.fileName}
            </div>
            {toast.totalBytes !== undefined && isCompleted && (
              <div className="text-[10px] font-mono text-tg-muted mt-0.5">
                {formatBytes(toast.totalBytes)}
              </div>
            )}
            {toast.errorMessage && !isCompleted && (
              <div className="text-[10px] font-mono text-tg-error/90 mt-0.5 line-clamp-2">
                {toast.errorMessage}
              </div>
            )}

            <div className="mt-2.5 flex items-center gap-1.5">
              {isCompleted ? (
                <>
                  <button
                    onClick={() => {
                      openFile(fullPath);
                      dismissToast(toast.id);
                    }}
                    className="flex items-center gap-1 rounded-md bg-tg-secondary px-2.5 py-1 text-[11px] font-semibold text-slate-950 hover:brightness-110"
                  >
                    <FileText className="h-3 w-3" /> Buka File
                  </button>
                  <button
                    onClick={() => {
                      openFolder(fullPath);
                      dismissToast(toast.id);
                    }}
                    className="flex items-center gap-1 rounded-md border border-tg-outline/40 bg-tg-high px-2.5 py-1 text-[11px] text-tg-ink hover:bg-tg-highest"
                  >
                    <FolderOpen className="h-3 w-3 text-tg-tertiary" /> Folder
                  </button>
                </>
              ) : (
                <button
                  onClick={() => {
                    resumeDownload(toast.downloadId);
                    dismissToast(toast.id);
                  }}
                  className="flex items-center gap-1 rounded-md bg-tg-primary-container px-2.5 py-1 text-[11px] font-semibold text-white hover:brightness-110"
                >
                  <RotateCcw className="h-3 w-3" /> Coba Lagi
                </button>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
};
