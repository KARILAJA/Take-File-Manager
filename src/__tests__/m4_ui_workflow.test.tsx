import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import App from "../App";
import { useAppStore, mockDownloads } from "../store";

describe("M4 UI Workflow: Tambah -> Progres -> Selesai -> Buka Folder", () => {
  beforeEach(() => {
    // Reset mock list
    mockDownloads.length = 0;

    // Reset store state
    useAppStore.setState({
      downloads: [],
      selectedId: null,
      filter: "all",
      searchQuery: "",
      theme: "dark",
      lang: "id",
      isAddModalOpen: false,
      isDetailModalOpen: false,
      completedModalItem: null,
    });
  });

  it("renders main UI elements (Toolbar, Sidebar, DownloadList, Status Bar)", async () => {
    await act(async () => {
      render(<App />);
    });

    // Check header / toolbar action
    expect(screen.getByText("Tambah URL")).toBeDefined();

    // Check sidebar navigation groups
    expect(screen.getByText("Semua Unduhan")).toBeDefined();
    expect(screen.getByText("Sedang Mengunduh")).toBeDefined();
    expect(screen.getByText("Selesai")).toBeDefined();

    // Check status bar
    expect(screen.getByText(/Siap \(Tauri 2 Engine Online\)/)).toBeDefined();
  });

  it("switches language between Indonesian and English correctly", async () => {
    await act(async () => {
      render(<App />);
    });

    const langBtn = screen.getByTitle("Ganti Bahasa / Switch Language");
    expect(screen.getByText("Tambah URL")).toBeDefined();

    // Switch to English
    await act(async () => {
      fireEvent.click(langBtn);
    });
    expect(screen.getByText("Add URL")).toBeDefined();
    expect(screen.getByText("All Downloads")).toBeDefined();

    // Switch back to Indonesian
    await act(async () => {
      fireEvent.click(langBtn);
    });
    expect(screen.getByText("Tambah URL")).toBeDefined();
    expect(screen.getByText("Semua Unduhan")).toBeDefined();
  });

  it("toggles dark and light theme class", async () => {
    let containerElement: HTMLElement;
    await act(async () => {
      const { container } = render(<App />);
      containerElement = container;
    });

    const themeBtn = screen.getByTitle("Toggle Theme");

    expect(containerElement!.firstChild).toHaveProperty("className");
    expect((containerElement!.firstChild as HTMLElement).className).toContain("dark");

    // Toggle to Light
    await act(async () => {
      fireEvent.click(themeBtn);
    });
    expect((containerElement!.firstChild as HTMLElement).className).not.toContain("dark");

    // Toggle back to Dark
    await act(async () => {
      fireEvent.click(themeBtn);
    });
    expect((containerElement!.firstChild as HTMLElement).className).toContain("dark");
  });

  it("completes full flow: Add URL -> Probing -> Add -> Progress -> Complete -> Open Folder", async () => {
    const openFolderSpy = vi.fn();
    useAppStore.setState({
      openFolder: async (path: string) => {
        openFolderSpy(path);
      },
    });

    await act(async () => {
      render(<App />);
    });

    // 1. Open Add URL Modal
    const addUrlBtn = screen.getByText("Tambah URL");
    await act(async () => {
      fireEvent.click(addUrlBtn);
    });

    expect(screen.getByText("Tambah Unduhan Baru")).toBeDefined();

    // 2. Input URL and Probe
    const urlInput = screen.getByPlaceholderText(/https:\/\/example\.com\/file\.zip/);
    await act(async () => {
      fireEvent.change(urlInput, {
        target: { value: "https://example.com/test_ubuntu_22.iso" },
      });
    });

    const probeBtn = screen.getByText("Cek URL");
    await act(async () => {
      fireEvent.click(probeBtn);
    });

    await waitFor(() => {
      expect(screen.getByText("Dukungan Resume:")).toBeDefined();
      expect(screen.getByText("Mendukung Resume")).toBeDefined();
    });

    // 3. Submit download
    const submitBtn = screen.getByText("Mulai Sekarang");
    await act(async () => {
      fireEvent.click(submitBtn);
    });

    // Modal should close
    await waitFor(() => {
      expect(screen.queryByText("Tambah Unduhan Baru")).toBeNull();
    });

    // 4. Verify added item is in DownloadList
    await waitFor(() => {
      expect(screen.getByText("test_ubuntu_22.iso")).toBeDefined();
      expect(screen.getByText("Mengunduh")).toBeDefined();
    });

    // 5. Complete download and trigger DownloadCompletedModal
    const completedItem = {
      id: "dl-completed-1",
      url: "https://example.com/test_ubuntu_22.iso",
      file_name: "test_ubuntu_22.iso",
      save_dir: "E:/Project/idm/downloads",
      total_bytes: 47185920,
      downloaded: 47185920,
      status: "completed",
      resumable: true,
      category: "others",
      progress_percent: 100.0,
      speed_bps: 0,
      created_at: Date.now(),
      completed_at: Date.now(),
    };

    await act(async () => {
      mockDownloads[0] = completedItem;
      useAppStore.setState({
        downloads: [completedItem],
        completedModalItem: completedItem,
      });
    });

    await waitFor(() => {
      expect(screen.getByText("Unduhan Selesai!")).toBeDefined();
      expect(screen.getByText("Buka Folder")).toBeDefined();
    });

    // 6. Click "Buka Folder"
    const openFolderBtn = screen.getByText("Buka Folder");
    await act(async () => {
      fireEvent.click(openFolderBtn);
    });

    expect(openFolderSpy).toHaveBeenCalledWith("E:/Project/idm/downloads/test_ubuntu_22.iso");

    // Modal closes after opening folder
    await waitFor(() => {
      expect(screen.queryByText("Unduhan Selesai!")).toBeNull();
    });
  });

  it("filters items by search query and category", async () => {
    const item1 = {
      id: "dl-1",
      url: "https://example.com/music.mp3",
      file_name: "song.mp3",
      save_dir: "E:/Project/idm/downloads",
      total_bytes: 1000,
      downloaded: 1000,
      status: "completed",
      resumable: true,
      category: "music",
      progress_percent: 100.0,
      speed_bps: 0,
      created_at: Date.now(),
    };
    const item2 = {
      id: "dl-2",
      url: "https://example.com/setup.zip",
      file_name: "archive.zip",
      save_dir: "E:/Project/idm/downloads",
      total_bytes: 5000,
      downloaded: 5000,
      status: "completed",
      resumable: true,
      category: "compressed",
      progress_percent: 100.0,
      speed_bps: 0,
      created_at: Date.now(),
    };

    mockDownloads.push(item1, item2);

    await act(async () => {
      render(<App />);
    });

    // Initially both should be visible in the table
    await waitFor(() => {
      expect(screen.getByText("song.mp3")).toBeDefined();
      expect(screen.getByText("archive.zip")).toBeDefined();
    });

    // Filter by search query "song"
    const searchInput = screen.getByPlaceholderText("Cari unduhan berdasarkan nama atau URL...");
    await act(async () => {
      fireEvent.change(searchInput, { target: { value: "song" } });
    });

    await waitFor(() => {
      expect(screen.getByText("song.mp3")).toBeDefined();
      expect(screen.queryByText("archive.zip")).toBeNull();
    });

    // Clear search
    await act(async () => {
      fireEvent.change(searchInput, { target: { value: "" } });
    });
    await waitFor(() => {
      expect(screen.getByText("song.mp3")).toBeDefined();
      expect(screen.getByText("archive.zip")).toBeDefined();
    });

    // Filter by category "Musik"
    const musicCatBtn = screen.getByText("Musik");
    await act(async () => {
      fireEvent.click(musicCatBtn);
    });

    await waitFor(() => {
      expect(screen.getByText("song.mp3")).toBeDefined();
      expect(screen.queryByText("archive.zip")).toBeNull();
    });
  });
});
