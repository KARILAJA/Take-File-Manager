import React, { useEffect, useState } from "react";
import { ShieldCheck, CheckCircle2, AlertCircle, Copy, X, FolderOpen } from "lucide-react";
import { useAppStore, FileChecksumResult } from "../store";
import { formatBytes, joinPath } from "../utils/format";

export const ChecksumModal: React.FC = () => {
  const {
    checksumTargetItem,
    setChecksumTargetItem,
    verifyFileChecksum,
    openFolder,
    t,
  } = useAppStore();

  const [algo, setAlgo] = useState<"sha256" | "sha512">("sha256");
  const [expected, setExpected] = useState("");
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<FileChecksumResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const runVerify = async (nextAlgo: "sha256" | "sha512", exp: string) => {
    if (!checksumTargetItem) return;
    setLoading(true);
    setError(null);
    try {
      const fullPath = joinPath(checksumTargetItem.save_dir, checksumTargetItem.file_name);
      const res = await verifyFileChecksum(fullPath, nextAlgo, exp || undefined);
      setResult(res);
    } catch (e) {
      setError(String(e));
      setResult(null);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (checksumTargetItem) {
      setResult(null);
      setError(null);
      runVerify(algo, expected);
    }
  }, [checksumTargetItem]);

  if (!checksumTargetItem) return null;

  const fullPath = joinPath(checksumTargetItem.save_dir, checksumTargetItem.file_name);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
      <div className="w-full max-w-lg rounded-xl border border-tg-outline/40 bg-tg-card shadow-2xl overflow-hidden text-xs">
        <div className="flex items-center justify-between border-b border-tg-outline/30 px-4 py-3 bg-tg-low">
          <div className="flex items-center gap-2 text-tg-ink font-semibold">
            <ShieldCheck className="h-4 w-4 text-tg-secondary" />
            <span>{t("checksum_title")}</span>
          </div>
          <button
            onClick={() => setChecksumTargetItem(null)}
            className="rounded-md p-1 text-tg-muted hover:bg-tg-high hover:text-tg-ink"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="p-4 space-y-3.5 text-tg-ink">
          <div className="rounded-lg border border-tg-outline/30 bg-tg-lowest p-3 space-y-1">
            <div className="font-semibold truncate">{checksumTargetItem.file_name}</div>
            <div className="font-mono text-[10px] text-tg-muted truncate">{fullPath}</div>
          </div>

          <div className="flex items-center gap-2">
            {(["sha256", "sha512"] as const).map((alg) => (
              <button
                key={alg}
                onClick={() => {
                  setAlgo(alg);
                  runVerify(alg, expected);
                }}
                className={`rounded-lg px-3 py-1.5 font-mono uppercase text-[11px] font-semibold border transition ${
                  algo === alg
                    ? "border-tg-primary bg-tg-primary-container text-white"
                    : "border-tg-outline/40 bg-tg-lowest text-tg-muted hover:text-tg-ink"
                }`}
              >
                {alg}
              </button>
            ))}
          </div>

          <div>
            <label className="block text-[11px] text-tg-muted mb-1">
              Hash Pembanding Opsional (Expected Hash)
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                value={expected}
                onChange={(e) => setExpected(e.target.value)}
                placeholder="Tempel hash SHA-256 / SHA-512 resmi untuk mencocokkan..."
                className="flex-1 rounded-lg border border-tg-outline/40 bg-tg-lowest px-3 py-1.5 font-mono text-[11px] text-tg-ink"
              />
              <button
                onClick={() => runVerify(algo, expected)}
                disabled={loading}
                className="rounded-lg bg-tg-primary-container px-3 py-1.5 font-semibold text-white hover:brightness-110 disabled:opacity-50"
              >
                {loading ? "Menghitung..." : "Verifikasi"}
              </button>
            </div>
          </div>

          {error && (
            <div className="rounded-lg border border-tg-error/40 bg-tg-error/15 px-3 py-2 text-tg-error font-mono">
              {error}
            </div>
          )}

          {result && (
            <div className="rounded-lg border border-tg-outline/30 bg-tg-lowest p-3 space-y-2">
              <div className="flex items-center justify-between text-[11px]">
                <span className="text-tg-muted font-mono">
                  {result.algorithm} • {formatBytes(result.file_size)} ({result.elapsed_ms} ms)
                </span>
                <button
                  onClick={() => navigator.clipboard?.writeText(result.hash)}
                  className="inline-flex items-center gap-1 text-tg-primary hover:underline"
                >
                  <Copy className="h-3 w-3" /> Salin Hash
                </button>
              </div>
              <div className="rounded bg-tg-card p-2 font-mono text-[11px] break-all text-tg-secondary border border-tg-outline/25">
                {result.hash}
              </div>

              {result.matches === true && (
                <div className="flex items-center gap-1.5 text-tg-secondary font-semibold">
                  <CheckCircle2 className="h-4 w-4" />
                  Hash COCOK! Integritas file 100% terverifikasi.
                </div>
              )}
              {result.matches === false && (
                <div className="flex items-center gap-1.5 text-tg-error font-semibold">
                  <AlertCircle className="h-4 w-4" />
                  PERINGATAN: Hash TIDAK cocok! File mungkin korup atau dimodifikasi.
                </div>
              )}
            </div>
          )}
        </div>

        <div className="flex justify-between border-t border-tg-outline/30 bg-tg-low px-4 py-3">
          <button
            onClick={() => openFolder(fullPath)}
            className="flex items-center gap-1.5 rounded-lg border border-tg-outline/40 bg-tg-high px-3 py-1.5 text-tg-ink hover:bg-tg-highest"
          >
            <FolderOpen className="h-3.5 w-3.5 text-tg-tertiary" />
            {t("btn_open_folder")}
          </button>
          <button
            onClick={() => setChecksumTargetItem(null)}
            className="rounded-lg bg-tg-high px-4 py-1.5 font-medium text-tg-ink hover:bg-tg-highest"
          >
            {t("btn_close")}
          </button>
        </div>
      </div>
    </div>
  );
};
