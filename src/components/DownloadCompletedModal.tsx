import React from "react";
import { CheckCircle2, FolderOpen, FileText, ShieldCheck, X } from "lucide-react";
import { useAppStore } from "../store";
import { formatBytes, joinPath } from "../utils/format";

export const DownloadCompletedModal: React.FC = () => {
  const {
    completedModalItem,
    setCompletedModalItem,
    setChecksumTargetItem,
    openFile,
    openFolder,
    t,
  } = useAppStore();

  if (!completedModalItem) return null;

  const fullPath = joinPath(completedModalItem.save_dir, completedModalItem.file_name);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
      <div className="w-full max-w-md rounded-xl border border-tg-outline/40 bg-tg-card shadow-2xl overflow-hidden text-xs">
        <div className="flex items-center justify-between border-b border-tg-outline/30 px-4 py-3 bg-tg-low">
          <div className="flex items-center gap-2 text-tg-secondary font-semibold">
            <CheckCircle2 className="h-4 w-4" />
            <span>{t("dialog_completed_title")}</span>
          </div>
          <button
            onClick={() => setCompletedModalItem(null)}
            className="rounded-md p-1 text-tg-muted hover:bg-tg-high hover:text-tg-ink"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="p-4 space-y-3">
          <div className="rounded-lg border border-tg-outline/30 bg-tg-lowest p-3">
            <h3 className="font-semibold text-tg-ink truncate mb-1">
              {completedModalItem.file_name}
            </h3>
            <p className="text-tg-muted font-mono text-[11px]">
              Ukuran: {formatBytes(completedModalItem.total_bytes || completedModalItem.downloaded)}
            </p>
            <p className="text-tg-muted font-mono text-[10px] truncate mt-1">
              Path: {fullPath}
            </p>
          </div>
        </div>

        <div className="flex flex-wrap justify-between gap-2 border-t border-tg-outline/30 bg-tg-low px-4 py-3">
          <button
            onClick={() => {
              const target = completedModalItem;
              setCompletedModalItem(null);
              setChecksumTargetItem(target);
            }}
            className="flex items-center gap-1 rounded-lg border border-tg-outline/40 bg-tg-high px-2.5 py-1.5 font-medium text-tg-muted hover:text-tg-ink"
          >
            <ShieldCheck className="h-3.5 w-3.5 text-tg-secondary" />
            SHA-256
          </button>
          <div className="flex items-center gap-2">
            <button
              onClick={() => {
                openFolder(fullPath);
                setCompletedModalItem(null);
              }}
              className="flex items-center gap-1.5 rounded-lg border border-tg-outline/40 bg-tg-high px-3 py-1.5 font-medium text-tg-ink hover:bg-tg-highest"
            >
              <FolderOpen className="h-3.5 w-3.5 text-tg-tertiary" />
              {t("btn_open_folder")}
            </button>
            <button
              onClick={() => {
                openFile(fullPath);
                setCompletedModalItem(null);
              }}
              className="flex items-center gap-1.5 rounded-lg bg-tg-secondary px-3 py-1.5 font-semibold text-slate-950 hover:brightness-110 shadow"
            >
              <FileText className="h-3.5 w-3.5" />
              {t("btn_open_file")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
