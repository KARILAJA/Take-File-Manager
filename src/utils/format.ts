export function formatBytes(bytes?: number): string {
  if (bytes === undefined || bytes === null || bytes <= 0) return "--";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let val = bytes;
  while (val >= 1024 && i < units.length - 1) {
    val /= 1024;
    i++;
  }
  return `${val.toFixed(i === 0 ? 0 : 2)} ${units[i]}`;
}

export function formatSpeed(bps?: number): string {
  if (!bps || bps <= 0) return "0 KB/s";
  return `${formatBytes(bps)}/s`;
}

export function formatEta(seconds?: number): string {
  if (!seconds || seconds <= 0 || !isFinite(seconds)) return "--";
  if (seconds < 60) return `${Math.round(seconds)}s`;
  const mins = Math.floor(seconds / 60);
  const secs = Math.round(seconds % 60);
  if (mins < 60) return `${mins}m ${secs}s`;
  const hours = Math.floor(mins / 60);
  const remMins = mins % 60;
  return `${hours}j ${remMins}m`;
}

export function formatDate(timestamp: number): string {
  if (!timestamp) return "--";
  const d = new Date(timestamp);
  return d.toLocaleDateString(undefined, {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function joinPath(dir: string, file: string): string {
  const cleanDir = (dir || "").replace(/[\\/]+$/, "");
  const sep = cleanDir.includes("\\") ? "\\" : "/";
  return cleanDir ? `${cleanDir}${sep}${file}` : file;
}
