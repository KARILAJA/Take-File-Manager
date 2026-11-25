import { describe, it, expect, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import App from "../App";
import { useAppStore, mockDownloads } from "../store";

describe("Stitch UI Features: Bulk Select, Grabber, Scheduler, Checksum, Delete & Toasts", () => {
  beforeEach(() => {
    mockDownloads.length = 0;
    useAppStore.setState({
      downloads: [],
      selectedId: null,
      selectedIds: [],
      activeView: "downloads",
      viewMode: "table",
      filter: "all",
      searchQuery: "",
      theme: "dark",
      lang: "id",
      isAddModalOpen: false,
      isDetailModalOpen: false,
      isSettingsModalOpen: false,
      completedModalItem: null,
      deleteModalTargetIds: null,
      checksumTargetItem: null,
      toasts: [],
    });
  });

  it("supports bulk select, delete confirmation dialog, and card/table view switch", async () => {
    mockDownloads.push(
      {
        id: "b-1",
        url: "https://example.com/a.zip",
        file_name: "a.zip",
        save_dir: "E:/Project/idm/downloads",
        total_bytes: 1024,
        downloaded: 1024,
        status: "completed",
        resumable: true,
        category: "compressed",
        progress_percent: 100,
        speed_bps: 0,
        created_at: Date.now(),
      },
      {
        id: "b-2",
        url: "https://example.com/b.iso",
        file_name: "b.iso",
        save_dir: "E:/Project/idm/downloads",
        total_bytes: 2048,
        downloaded: 1024,
        status: "paused",
        resumable: true,
        category: "compressed",
        progress_percent: 50,
        speed_bps: 0,
        created_at: Date.now(),
      }
    );

    await act(async () => {
      render(<App />);
    });

    await waitFor(() => {
      expect(screen.getAllByText("Take File Manager").length).toBeGreaterThanOrEqual(1);
      expect(screen.getByAltText("Take File Manager Logo")).toBeDefined();
      expect(screen.getByLabelText("Minimize")).toBeDefined();
      expect(screen.getByLabelText("Maximize")).toBeDefined();
      expect(screen.getByLabelText("Close")).toBeDefined();
      expect(screen.getByText("a.zip")).toBeDefined();
      expect(screen.getByText("b.iso")).toBeDefined();
    });

    // Switch to Card view and back to Table
    await act(async () => {
      fireEvent.click(screen.getByTitle("Kartu"));
    });
    expect(screen.getByText("a.zip")).toBeDefined();
    await act(async () => {
      fireEvent.click(screen.getByTitle("Tabel"));
    });

    // Select all visible
    await act(async () => {
      fireEvent.click(screen.getByText("Checklist Semua"));
    });
    expect(screen.getByText("2 terpilih")).toBeDefined();

    // Open Delete Confirmation Modal
    await act(async () => {
      fireEvent.click(screen.getByText("Hapus Terpilih"));
    });
    expect(screen.getByText(/Konfirmasi Hapus Unduhan \(2\)/)).toBeDefined();

    // Confirm deletion
    await act(async () => {
      fireEvent.click(screen.getByText("Ya, Hapus"));
    });

    await waitFor(() => {
      expect(mockDownloads.length).toBe(0);
    });
  });

  it("opens Batch Grabber & Smart Scheduler views and Checksum Inspector", async () => {
    const completed = {
      id: "chk-1",
      url: "https://example.com/firmware.bin",
      file_name: "firmware.bin",
      save_dir: "E:/Project/idm/downloads",
      total_bytes: 1048576,
      downloaded: 1048576,
      status: "completed",
      resumable: true,
      category: "programs",
      progress_percent: 100,
      speed_bps: 0,
      created_at: Date.now(),
    };
    mockDownloads.push(completed);

    await act(async () => {
      render(<App />);
    });

    // Navigate to Batch Grabber
    await act(async () => {
      fireEvent.click(screen.getByText("Proyek Grabber"));
    });
    expect(screen.getByText(/Batch Downloader & Site Grabber/)).toBeDefined();

    // Test pattern preview
    const textarea = screen.getByPlaceholderText(/episode-\[01-05\]\.mp4/);
    await act(async () => {
      fireEvent.change(textarea, {
        target: { value: "https://cdn.example.com/file-[01-03].zip" },
      });
    });
    await act(async () => {
      fireEvent.click(screen.getByText("Pratinjau Pola"));
    });
    await waitFor(() => {
      expect(screen.getByText(/3 URL siap dari pola/)).toBeDefined();
    });

    // Navigate to Smart Scheduler
    await act(async () => {
      fireEvent.click(screen.getByText("Antrean & Penjadwal"));
    });
    expect(screen.getByText(/Smart Scheduler & Queues/)).toBeDefined();
    expect(screen.getByText("Antrean Utama")).toBeDefined();

    // Return to Downloads & trigger Checksum Inspector
    await act(async () => {
      fireEvent.click(screen.getByText("Kembali"));
    });

    await act(async () => {
      useAppStore.setState({ checksumTargetItem: completed });
    });

    await waitFor(() => {
      expect(screen.getByText("Inspektor Integritas Checksum")).toBeDefined();
      expect(
        screen.getByText("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
      ).toBeDefined();
    });

    // Verify toast notification rendering
    await act(async () => {
      useAppStore.setState({
        toasts: [
          {
            id: "t-fail",
            downloadId: "b-fail",
            kind: "failed",
            fileName: "broken.iso",
            saveDir: "E:/Project/idm/downloads",
            errorMessage: "Socket timeout",
            createdAt: Date.now(),
          },
        ],
      });
    });
    expect(screen.getByText("Unduhan Terputus / Gagal")).toBeDefined();
    expect(screen.getByText("broken.iso")).toBeDefined();
  });
});
