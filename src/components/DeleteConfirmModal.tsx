import React, { useState } from "react";
import { Trash2, AlertTriangle, X } from "lucide-react";
import { useAppStore } from "../store";

export const DeleteConfirmModal: React.FC = () => {
  const {
    downloads,
    deleteModalTargetIds,
    setDeleteModalTargetIds,
    bulkDelete,
    t,
  } = useAppStore();

  const [deleteFromDisk, setDeleteFromDisk] = useState(false);
  const [deleting, setDeleting] = useState(false);

  if (!deleteModalTargetIds || deleteModalTargetIds.length === 0) return null;

  const targetItems = downloads.filter((d) => deleteModalTargetIds.includes(d.id));
  const count = targetItems.length || deleteModalTargetIds.length;

  const handleConfirm = async () => {
    setDeleting(true);
    try {
      await bulkDelete(deleteModalTargetIds, deleteFromDisk);
      setDeleteModalTargetIds(null);
      setDeleteFromDisk(false);
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
      <div className="w-full max-w-md rounded-xl border border-tg-error/40 bg-tg-card shadow-2xl overflow-hidden text-xs">
        <div className="flex items-center justify-between border-b border-tg-outline/30 px-4 py-3 bg-tg-low">
          <div className="flex items-center gap-2 text-tg-error font-semibold">
            <AlertTriangle className="h-4 w-4" />
            <span>Konfirmasi Hapus Unduhan ({count})</span>
          </div>
          <button
            onClick={() => setDeleteModalTargetIds(null)}
            className="rounded-md p-1 text-tg-muted hover:bg-tg-high hover:text-tg-ink"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="p-4 space-y-3 text-tg-ink">
          <p className="text-tg-muted">
            {count === 1 && targetItems[0]
              ? `Hapus "${targetItems[0].file_name}" dari Take File Manager?`
              : `Hapus ${count} unduhan terpilih dari Take File Manager?`}
          </p>

          <div className="space-y-2">
            <label
              className={`flex items-start gap-2.5 rounded-lg border p-3 cursor-pointer transition ${
                !deleteFromDisk
                  ? "border-tg-primary bg-tg-primary-container/15"
                  : "border-tg-outline/30 bg-tg-lowest"
              }`}
            >
              <input
                type="radio"
                name="purgeMode"
                checked={!deleteFromDisk}
                onChange={() => setDeleteFromDisk(false)}
                className="mt-0.5 accent-tg-primary-container"
              />
              <div>
                <div className="font-semibold text-tg-ink">Hapus dari daftar saja</div>
                <div className="text-[11px] text-tg-muted">
                  Berkas di disk tetap aman dan tidak dihapus.
                </div>
              </div>
            </label>

            <label
              className={`flex items-start gap-2.5 rounded-lg border p-3 cursor-pointer transition ${
                deleteFromDisk
                  ? "border-tg-error bg-tg-error/15"
                  : "border-tg-outline/30 bg-tg-lowest"
              }`}
            >
              <input
                type="radio"
                name="purgeMode"
                checked={deleteFromDisk}
                onChange={() => setDeleteFromDisk(true)}
                className="mt-0.5 accent-tg-error"
              />
              <div>
                <div className="font-semibold text-tg-error">Hapus permanen beserta file di disk</div>
                <div className="text-[11px] text-tg-muted">
                  Tindakan ini menghapus berkas fisik secara permanen.
                </div>
              </div>
            </label>
          </div>
        </div>

        <div className="flex justify-end gap-2 border-t border-tg-outline/30 bg-tg-low px-4 py-3">
          <button
            onClick={() => setDeleteModalTargetIds(null)}
            className="rounded-lg px-3 py-1.5 text-tg-muted hover:bg-tg-high hover:text-tg-ink"
          >
            {t("btn_cancel")}
          </button>
          <button
            disabled={deleting}
            onClick={handleConfirm}
            className="flex items-center gap-1.5 rounded-lg bg-tg-error px-4 py-1.5 font-semibold text-slate-950 hover:brightness-110 disabled:opacity-50"
          >
            <Trash2 className="h-3.5 w-3.5" />
            {deleting ? "Menghapus..." : "Ya, Hapus"}
          </button>
        </div>
      </div>
    </div>
  );
};
