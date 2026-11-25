import React, { useEffect, useState } from "react";
import {
  CalendarClock,
  Play,
  Pause,
  Plus,
  Trash2,
  Save,
  ArrowLeft,
  ArrowUp,
  ArrowDown,
  Power,
} from "lucide-react";
import { useAppStore, QueueRecord, ScheduleRecord } from "../store";

const DAYS = [
  { label: "Min", bit: 1 },
  { label: "Sen", bit: 2 },
  { label: "Sel", bit: 4 },
  { label: "Rab", bit: 8 },
  { label: "Kam", bit: 16 },
  { label: "Jum", bit: 32 },
  { label: "Sab", bit: 64 },
];

export const SchedulerQueuesView: React.FC = () => {
  const {
    queues,
    schedules,
    downloads,
    setActiveView,
    loadQueuesAndSchedules,
    createQueue,
    updateQueue,
    deleteQueue,
    startQueue,
    pauseQueue,
    createSchedule,
    updateSchedule,
    setDownloadPriority,
  } = useAppStore();

  const [selectedQueueId, setSelectedQueueId] = useState("default");
  const [newQueueName, setNewQueueName] = useState("");
  const [notice, setNotice] = useState<string | null>(null);

  // Schedule form for selectedQueueId
  const [daysMask, setDaysMask] = useState(127);
  const [startTime, setStartTime] = useState("01:00");
  const [stopTime, setStopTime] = useState("06:00");
  const [afterAction, setAfterAction] = useState("none");
  const [schedEnabled, setSchedEnabled] = useState(false);

  useEffect(() => {
    loadQueuesAndSchedules();
  }, [loadQueuesAndSchedules]);

  useEffect(() => {
    if (queues.length > 0 && !queues.some((q) => q.id === selectedQueueId)) {
      setSelectedQueueId(queues[0].id);
    }
  }, [queues, selectedQueueId]);

  useEffect(() => {
    const existing = schedules.find((s) => s.queue_id === selectedQueueId);
    if (existing) {
      setDaysMask(existing.days_mask);
      setStartTime(existing.start_time || "01:00");
      setStopTime(existing.stop_time || "06:00");
      setAfterAction(existing.after_action || "none");
      setSchedEnabled(existing.enabled);
    } else {
      setDaysMask(127);
      setStartTime("01:00");
      setStopTime("06:00");
      setAfterAction("none");
      setSchedEnabled(false);
    }
  }, [selectedQueueId, schedules]);

  const activeQueue = queues.find((q) => q.id === selectedQueueId);

  const handleCreateQueue = async () => {
    if (!newQueueName.trim()) return;
    const id = `q-${Date.now()}`;
    await createQueue({
      id,
      name: newQueueName.trim(),
      max_concurrent: 3,
      speed_limit: null,
      enabled: true,
    });
    setNewQueueName("");
    setSelectedQueueId(id);
    setNotice("Antrean baru berhasil dibuat.");
  };

  const handleSaveConcurrency = async (q: QueueRecord, maxConcurrent: number) => {
    await updateQueue({ ...q, max_concurrent: Math.max(1, Math.min(16, maxConcurrent)) });
    setNotice("Pengaturan konkurensi antrean diperbarui.");
  };

  const handleSaveSchedule = async () => {
    const existing = schedules.find((s) => s.queue_id === selectedQueueId);
    const payload: ScheduleRecord = {
      id: existing ? existing.id : `sched-${selectedQueueId}`,
      queue_id: selectedQueueId,
      days_mask: daysMask,
      start_time: startTime || null,
      stop_time: stopTime || null,
      after_action: afterAction,
      after_command: null,
      enabled: schedEnabled,
    };
    if (existing) {
      await updateSchedule(payload);
    } else {
      await createSchedule(payload);
    }
    setNotice("Jadwal antrean disimpan.");
  };

  const pendingDownloads = downloads.filter((d) => d.status !== "completed");

  return (
    <div className="flex flex-1 flex-col overflow-hidden bg-tg-bg p-4 text-xs">
      <div className="flex items-center justify-between border-b border-tg-outline/30 pb-3 mb-3">
        <div className="flex items-center gap-3">
          <button
            onClick={() => setActiveView("downloads")}
            className="flex items-center gap-1 rounded-lg border border-tg-outline/40 bg-tg-card px-2.5 py-1.5 text-tg-ink hover:bg-tg-high"
          >
            <ArrowLeft className="h-3.5 w-3.5" />
            <span>Kembali</span>
          </button>
          <div>
            <h1 className="text-sm font-bold text-tg-ink flex items-center gap-2">
              <CalendarClock className="h-4 w-4 text-tg-primary" />
              Smart Scheduler &amp; Queues
            </h1>
            <p className="text-[11px] text-tg-muted">
              Atur jadwal mulai/berhenti otomatis, batas unduhan serentak per antrean, dan aksi pasca-unduhan OS.
            </p>
          </div>
        </div>
      </div>

      {notice && (
        <div className="mb-3 rounded-lg border border-tg-secondary/40 bg-tg-secondary/15 px-3 py-2 text-tg-secondary font-medium">
          {notice}
        </div>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-12 gap-4 flex-1 overflow-auto">
        {/* Column 1: Queues List */}
        <div className="lg:col-span-4 rounded-xl border border-tg-outline/30 bg-tg-card p-3 flex flex-col justify-between space-y-3">
          <div className="space-y-2">
            <div className="font-semibold text-tg-ink">Daftar Antrean ({queues.length})</div>
            <div className="space-y-1.5">
              {queues.map((q) => {
                const selected = q.id === selectedQueueId;
                return (
                  <div
                    key={q.id}
                    onClick={() => setSelectedQueueId(q.id)}
                    className={`rounded-lg border p-2.5 cursor-pointer transition ${
                      selected
                        ? "border-tg-primary bg-tg-primary-container/15"
                        : "border-tg-outline/30 bg-tg-lowest hover:bg-tg-high/60"
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <span className="font-semibold text-tg-ink">{q.name}</span>
                      <span className="font-mono text-[10px] text-tg-primary">
                        Max {q.max_concurrent} aktif
                      </span>
                    </div>
                    <div className="mt-2 flex items-center gap-1.5" onClick={(e) => e.stopPropagation()}>
                      <button
                        onClick={() => startQueue(q.id)}
                        className="flex items-center gap-1 rounded bg-tg-secondary/20 px-2 py-1 text-[10px] font-semibold text-tg-secondary hover:bg-tg-secondary/30"
                      >
                        <Play className="h-3 w-3" /> Mulai
                      </button>
                      <button
                        onClick={() => pauseQueue(q.id)}
                        className="flex items-center gap-1 rounded bg-tg-tertiary/20 px-2 py-1 text-[10px] font-semibold text-tg-tertiary hover:bg-tg-tertiary/30"
                      >
                        <Pause className="h-3 w-3" /> Jeda
                      </button>
                      {q.id !== "default" && (
                        <button
                          onClick={() => deleteQueue(q.id)}
                          className="ml-auto rounded p-1 text-tg-error hover:bg-tg-error/20"
                          title="Hapus antrean"
                        >
                          <Trash2 className="h-3 w-3" />
                        </button>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          <div className="pt-2 border-t border-tg-outline/30 space-y-2">
            <label className="block text-[11px] text-tg-muted">Buat Antrean Baru</label>
            <div className="flex gap-1.5">
              <input
                type="text"
                value={newQueueName}
                onChange={(e) => setNewQueueName(e.target.value)}
                placeholder="Nama antrean (mis. Malam)"
                className="flex-1 rounded-lg border border-tg-outline/40 bg-tg-lowest px-2.5 py-1.5 text-tg-ink"
              />
              <button
                onClick={handleCreateQueue}
                disabled={!newQueueName.trim()}
                className="flex items-center gap-1 rounded-lg bg-tg-primary-container px-3 py-1.5 font-semibold text-white hover:brightness-110 disabled:opacity-40"
              >
                <Plus className="h-3.5 w-3.5" /> Buat
              </button>
            </div>
          </div>
        </div>

        {/* Column 2: Schedule & Trigger Rules */}
        <div className="lg:col-span-4 rounded-xl border border-tg-outline/30 bg-tg-card p-3 space-y-3.5">
          <div className="flex items-center justify-between">
            <span className="font-semibold text-tg-ink">Aturan Penjadwalan Otomatis</span>
            <label className="flex items-center gap-1.5 text-[11px] font-semibold text-tg-secondary cursor-pointer">
              <input
                type="checkbox"
                checked={schedEnabled}
                onChange={(e) => setSchedEnabled(e.target.checked)}
                className="accent-tg-secondary"
              />
              Aktifkan Jadwal
            </label>
          </div>

          {activeQueue && (
            <div className="rounded-lg border border-tg-outline/30 bg-tg-lowest p-2.5 space-y-1.5">
              <div className="flex justify-between items-center">
                <span className="text-tg-muted">Batas Unduhan Serentak ({activeQueue.name})</span>
                <span className="font-mono font-bold text-tg-primary">{activeQueue.max_concurrent}</span>
              </div>
              <input
                type="range"
                min={1}
                max={10}
                value={activeQueue.max_concurrent}
                onChange={(e) => handleSaveConcurrency(activeQueue, parseInt(e.target.value, 10))}
                className="w-full accent-tg-primary-container"
              />
            </div>
          )}

          <div>
            <div className="text-[11px] text-tg-muted mb-1.5">Hari Aktif</div>
            <div className="flex flex-wrap gap-1">
              {DAYS.map((d) => {
                const active = (daysMask & d.bit) !== 0;
                return (
                  <button
                    key={d.bit}
                    type="button"
                    onClick={() => setDaysMask(active ? daysMask & ~d.bit : daysMask | d.bit)}
                    className={`rounded-md px-2 py-1 font-medium text-[11px] border transition ${
                      active
                        ? "border-tg-primary bg-tg-primary-container text-white"
                        : "border-tg-outline/40 bg-tg-lowest text-tg-muted"
                    }`}
                  >
                    {d.label}
                  </button>
                );
              })}
            </div>
          </div>

          <div className="grid grid-cols-2 gap-2.5">
            <div>
              <label className="block text-[11px] text-tg-muted mb-1">Jam Mulai</label>
              <input
                type="time"
                value={startTime}
                onChange={(e) => setStartTime(e.target.value)}
                className="w-full rounded-lg border border-tg-outline/40 bg-tg-lowest px-2.5 py-1.5 font-mono text-tg-ink"
              />
            </div>
            <div>
              <label className="block text-[11px] text-tg-muted mb-1">Jam Berhenti</label>
              <input
                type="time"
                value={stopTime}
                onChange={(e) => setStopTime(e.target.value)}
                className="w-full rounded-lg border border-tg-outline/40 bg-tg-lowest px-2.5 py-1.5 font-mono text-tg-ink"
              />
            </div>
          </div>

          <div>
            <label className="flex items-center gap-1.5 text-[11px] text-tg-muted mb-1">
              <Power className="h-3.5 w-3.5 text-tg-tertiary" />
              Aksi Otomatis Setelah Selesai
            </label>
            <select
              value={afterAction}
              onChange={(e) => setAfterAction(e.target.value)}
              className="w-full rounded-lg border border-tg-outline/40 bg-tg-lowest px-2.5 py-1.5 text-tg-ink"
            >
              <option value="none">Tidak ada (Biarkan menyala)</option>
              <option value="sleep">Tidurkan Komputer (Sleep)</option>
              <option value="hibernate">Hibernasi Sistem</option>
              <option value="shutdown">Matikan Komputer (Shutdown)</option>
            </select>
          </div>

          <button
            onClick={handleSaveSchedule}
            className="w-full flex items-center justify-center gap-1.5 rounded-lg bg-tg-primary-container px-4 py-2 font-semibold text-white hover:brightness-110"
          >
            <Save className="h-3.5 w-3.5" />
            Simpan Jadwal Antrean
          </button>
        </div>

        {/* Column 3: Pending Queue Priority Items */}
        <div className="lg:col-span-4 rounded-xl border border-tg-outline/30 bg-tg-card p-3 flex flex-col">
          <div className="font-semibold text-tg-ink mb-2">
            Prioritas Tugas Belum Selesai ({pendingDownloads.length})
          </div>
          <div className="flex-1 overflow-auto space-y-1.5">
            {pendingDownloads.length === 0 ? (
              <p className="py-10 text-center text-tg-muted">
                Tidak ada unduhan yang sedang antre atau tertunda.
              </p>
            ) : (
              pendingDownloads.map((d) => (
                <div
                  key={d.id}
                  className="flex items-center justify-between gap-2 rounded-lg border border-tg-outline/30 bg-tg-lowest px-2.5 py-2"
                >
                  <div className="min-w-0">
                    <div className="font-medium text-tg-ink truncate">{d.file_name}</div>
                    <div className="text-[10px] font-mono text-tg-muted uppercase">
                      {d.status} • {d.progress_percent.toFixed(0)}%
                    </div>
                  </div>
                  <div className="flex items-center gap-1 flex-shrink-0">
                    <button
                      onClick={() => setDownloadPriority(d.id, 10)}
                      title="Naikkan prioritas"
                      className="rounded p-1 bg-tg-high hover:text-tg-secondary"
                    >
                      <ArrowUp className="h-3 w-3" />
                    </button>
                    <button
                      onClick={() => setDownloadPriority(d.id, 0)}
                      title="Turunkan prioritas"
                      className="rounded p-1 bg-tg-high hover:text-tg-tertiary"
                    >
                      <ArrowDown className="h-3 w-3" />
                    </button>
                  </div>
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
