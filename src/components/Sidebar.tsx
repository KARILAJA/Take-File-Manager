import React from "react";
import {
  Layers,
  ArrowDownCircle,
  CheckCircle,
  PauseCircle,
  Clock,
  Archive,
  FileText,
  Music,
  Cpu,
  Video,
  Image as ImageIcon,
  Magnet,
  Folder,
  Radar,
  CalendarClock,
} from "lucide-react";
import { useAppStore } from "../store";

export const Sidebar: React.FC = () => {
  const { filter, activeView, setFilter, setActiveView, downloads, queues, t } = useAppStore();

  const countByStatus = (status: string) => downloads.filter((d) => d.status === status).length;
  const countByCategory = (cat: string) => downloads.filter((d) => d.category === cat).length;

  const unfinishedCount = downloads.filter((d) => d.status !== "completed").length;

  const statusItems = [
    { id: "all", label: t("all_downloads"), icon: Layers, count: downloads.length },
    {
      id: "downloading",
      label: t("downloading"),
      icon: ArrowDownCircle,
      count: countByStatus("downloading") + countByStatus("probing"),
    },
    { id: "unfinished", label: t("unfinished"), icon: Clock, count: unfinishedCount },
    { id: "completed", label: t("completed"), icon: CheckCircle, count: countByStatus("completed") },
    {
      id: "paused",
      label: t("paused_failed"),
      icon: PauseCircle,
      count: countByStatus("paused") + countByStatus("failed") + countByStatus("waiting_refresh"),
    },
  ];

  const categoryItems = [
    { id: "compressed", label: t("compressed"), icon: Archive, count: countByCategory("compressed") },
    { id: "documents", label: t("documents"), icon: FileText, count: countByCategory("documents") },
    { id: "music", label: t("music"), icon: Music, count: countByCategory("music") },
    { id: "programs", label: t("programs"), icon: Cpu, count: countByCategory("programs") },
    { id: "video", label: t("video"), icon: Video, count: countByCategory("video") },
    { id: "images", label: t("images"), icon: ImageIcon, count: countByCategory("images") },
    { id: "torrent", label: t("torrent"), icon: Magnet, count: countByCategory("torrent") },
    { id: "others", label: t("others"), icon: Folder, count: countByCategory("others") },
  ];

  return (
    <aside className="w-56 flex-shrink-0 border-r border-tg-outline/30 bg-tg-low p-2.5 select-none text-xs flex flex-col justify-between overflow-y-auto">
      <div className="space-y-4">
        {/* Views & Tools (Stitch Navigation) */}
        <div>
          <div className="px-2 pb-1.5 text-[10px] font-bold uppercase tracking-wider text-tg-muted/70">
            Status
          </div>
          <div className="space-y-0.5">
            {statusItems.map((item) => {
              const Icon = item.icon;
              const active = activeView === "downloads" && filter === item.id;
              return (
                <button
                  key={item.id}
                  onClick={() => setFilter(item.id)}
                  className={`flex w-full items-center justify-between rounded-md px-2.5 py-1.5 transition-colors ${
                    active
                      ? "bg-tg-primary-container/20 text-tg-primary font-semibold border border-tg-primary/30 shadow-sm"
                      : "text-tg-muted hover:bg-tg-high/70 hover:text-tg-ink border border-transparent"
                  }`}
                >
                  <div className="flex items-center gap-2 truncate">
                    <Icon className="h-3.5 w-3.5 flex-shrink-0" />
                    <span className="truncate">{item.label}</span>
                  </div>
                  <span
                    className={`rounded px-1.5 py-0.5 font-mono text-[10px] tabular-nums ${
                      active
                        ? "bg-tg-primary-container text-white"
                        : "bg-tg-high text-tg-muted"
                    }`}
                  >
                    {item.count}
                  </span>
                </button>
              );
            })}
          </div>
        </div>

        {/* Workspace Modules */}
        <div>
          <div className="px-2 pb-1.5 text-[10px] font-bold uppercase tracking-wider text-tg-muted/70">
            Engine Modules
          </div>
          <div className="space-y-0.5">
            <button
              onClick={() => setActiveView("grabber")}
              className={`flex w-full items-center justify-between rounded-md px-2.5 py-1.5 transition-colors ${
                activeView === "grabber"
                  ? "bg-tg-primary-container/20 text-tg-primary font-semibold border border-tg-primary/30"
                  : "text-tg-muted hover:bg-tg-high/70 hover:text-tg-ink border border-transparent"
              }`}
            >
              <div className="flex items-center gap-2 truncate">
                <Radar className="h-3.5 w-3.5 flex-shrink-0" />
                <span className="truncate">{t("grabber_projects")}</span>
              </div>
            </button>
            <button
              onClick={() => setActiveView("scheduler")}
              className={`flex w-full items-center justify-between rounded-md px-2.5 py-1.5 transition-colors ${
                activeView === "scheduler"
                  ? "bg-tg-primary-container/20 text-tg-primary font-semibold border border-tg-primary/30"
                  : "text-tg-muted hover:bg-tg-high/70 hover:text-tg-ink border border-transparent"
              }`}
            >
              <div className="flex items-center gap-2 truncate">
                <CalendarClock className="h-3.5 w-3.5 flex-shrink-0" />
                <span className="truncate">{t("queues_scheduler")}</span>
              </div>
              {queues.length > 0 && (
                <span className="rounded bg-tg-high px-1.5 py-0.5 font-mono text-[10px] text-tg-muted">
                  {queues.length}
                </span>
              )}
            </button>
          </div>
        </div>

        {/* File Types / Categories */}
        <div>
          <div className="px-2 pb-1.5 text-[10px] font-bold uppercase tracking-wider text-tg-muted/70">
            {t("categories")}
          </div>
          <div className="space-y-0.5">
            {categoryItems.map((item) => {
              const Icon = item.icon;
              const active = activeView === "downloads" && filter === item.id;
              return (
                <button
                  key={item.id}
                  onClick={() => setFilter(item.id)}
                  className={`flex w-full items-center justify-between rounded-md px-2.5 py-1.5 transition-colors ${
                    active
                      ? "bg-tg-primary-container/20 text-tg-primary font-semibold border border-tg-primary/30"
                      : "text-tg-muted hover:bg-tg-high/70 hover:text-tg-ink border border-transparent"
                  }`}
                >
                  <div className="flex items-center gap-2 truncate">
                    <Icon className="h-3.5 w-3.5 flex-shrink-0" />
                    <span className="truncate">{item.label}</span>
                  </div>
                  <span
                    className={`rounded px-1.5 py-0.5 font-mono text-[10px] tabular-nums ${
                      active
                        ? "bg-tg-primary-container text-white"
                        : "bg-tg-high text-tg-muted"
                    }`}
                  >
                    {item.count}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      </div>

      <div className="mt-4 pt-2 border-t border-tg-outline/30 px-1 text-[10px] text-tg-muted flex justify-between items-center">
        <span className="truncate font-medium">Take File Manager</span>
        <span className="font-mono text-tg-secondary font-semibold">v0.1.0</span>
      </div>
    </aside>
  );
};
