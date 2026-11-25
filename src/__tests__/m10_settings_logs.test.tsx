import { describe, it, expect, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import App from "../App";
import { useAppStore, mockDownloads } from "../store";

describe("M10 UI: Settings & Log Viewer Modal", () => {
  beforeEach(() => {
    mockDownloads.length = 0;
    useAppStore.setState({
      downloads: [],
      selectedId: null,
      filter: "all",
      searchQuery: "",
      theme: "dark",
      lang: "id",
      isAddModalOpen: false,
      isDetailModalOpen: false,
      isSettingsModalOpen: false,
      completedModalItem: null,
      networkSettings: {
        proxy_mode: "system",
        max_connections_per_host: 6,
        auto_update_enabled: true,
        speed_limit_kbps: 0,
      },
      agentPolicy: {
        mode: "ask",
        allowed_dirs: [],
        block_private_ips: true,
        max_requests_per_minute: 30,
      },
      auditLogs: [],
    });
  });

  it("opens Settings & Log Viewer, saves network proxy & agent policy, and renders audit logs", async () => {
    await act(async () => {
      render(<App />);
    });

    expect(screen.getByText("Torrent")).toBeDefined();
    const settingsBtn = screen.getByTitle("Pengaturan & Log");
    await act(async () => {
      fireEvent.click(settingsBtn);
    });

    expect(screen.getByText(/Pengaturan & Log Viewer/)).toBeDefined();

    // Wait for initial loadSettingsAndLogs() to populate agent allowed dirs & logs
    await waitFor(() => {
      expect(screen.getByText(/Log & Audit \(1\)/)).toBeDefined();
    });

    // 1. Switch Proxy Mode to Manual and save
    const proxySelect = screen.getByDisplayValue("Ikuti Sistem (Windows Proxy)");
    await act(async () => {
      fireEvent.change(proxySelect, { target: { value: "manual" } });
    });

    const proxyInput = screen.getByPlaceholderText("http://127.0.0.1:8888");
    await act(async () => {
      fireEvent.change(proxyInput, { target: { value: "http://127.0.0.1:8888" } });
    });

    await act(async () => {
      fireEvent.click(screen.getByText("Simpan Pengaturan Jaringan"));
    });

    expect(screen.getByText("Pengaturan jaringan disimpan")).toBeDefined();
    expect(useAppStore.getState().networkSettings.proxy_mode).toBe("manual");
    expect(useAppStore.getState().networkSettings.proxy_url).toBe("http://127.0.0.1:8888");

    // 2. Switch to Akses Agent tab and save policy
    await act(async () => {
      fireEvent.click(screen.getByText("Akses Agent (MCP/CLI)"));
    });

    const agentSelect = screen.getByDisplayValue("Tanya Setiap Kali (Default / Ask)");
    await act(async () => {
      fireEvent.change(agentSelect, { target: { value: "allowlist" } });
    });

    const dirsTextarea = screen.getByDisplayValue("E:/Project/idm/downloads");
    await act(async () => {
      fireEvent.change(dirsTextarea, {
        target: { value: "E:/Project/idm/downloads\nD:/SafeDownloads" },
      });
    });

    await act(async () => {
      fireEvent.click(screen.getByText("Simpan Kebijakan Agent"));
    });

    expect(screen.getByText("Kebijakan akses agent disimpan")).toBeDefined();
    expect(useAppStore.getState().agentPolicy.mode).toBe("allowlist");
    expect(useAppStore.getState().agentPolicy.allowed_dirs).toEqual([
      "E:/Project/idm/downloads",
      "D:/SafeDownloads",
    ]);

    // 3. Switch to Log & Audit tab and verify logs render
    await act(async () => {
      fireEvent.click(screen.getByText(/Log & Audit/));
    });

    expect(screen.getByText("TurboGet Core Engine initialized")).toBeDefined();

    // Inject multiple log levels and confirm rendering
    await act(async () => {
      useAppStore.setState({
        auditLogs: [
          { id: 1, ts: Date.now(), level: "INFO", message: "Batch completed" },
          { id: 2, ts: Date.now(), level: "AGENT_AUDIT", message: "MCP tool add_download allowed" },
          { id: 3, ts: Date.now(), level: "ERROR", message: "407 Proxy Auth failed" },
        ],
      });
    });

    expect(screen.getByText("AGENT_AUDIT")).toBeDefined();
    expect(screen.getByText("MCP tool add_download allowed")).toBeDefined();
    expect(screen.getByText("407 Proxy Auth failed")).toBeDefined();

    // 4. Close modal
    await act(async () => {
      fireEvent.click(screen.getByLabelText("Close Settings"));
    });
    expect(screen.queryByText(/Pengaturan & Log Viewer/)).toBeNull();

    // 5. Toolbar Speed Limiter & row number column
    const speedLimitSelect = screen.getByLabelText("Limiter Kecepatan");
    await act(async () => {
      fireEvent.change(speedLimitSelect, { target: { value: "512" } });
    });
    expect(useAppStore.getState().networkSettings.speed_limit_kbps).toBe(512);

    // 6. Browser Integration & AutoStart menu
    await act(async () => {
      fireEvent.click(screen.getByTitle("Integrasi Ekstensi Browser (Chrome, Edge, Opera, Firefox)"));
    });
    await waitFor(() => {
      expect(screen.getByText(/Peringatan Wajib: Aktifkan Developer Mode/)).toBeDefined();
      expect(screen.getAllByText("Google Chrome").length).toBeGreaterThanOrEqual(1);
      expect(screen.getAllByText("Microsoft Edge").length).toBeGreaterThanOrEqual(1);
      expect(screen.getByText("Opera Browser")).toBeDefined();
      expect(screen.getByText("Mozilla Firefox")).toBeDefined();
      expect(screen.getAllByText("Soon on Store Addon").length).toBeGreaterThanOrEqual(1);
    });

    await act(async () => {
      fireEvent.click(screen.getByText(/Jaringan & Startup/));
    });
    const autoCheckbox = screen.getByLabelText("AutoStart Windows") as HTMLInputElement;
    expect(autoCheckbox.checked).toBe(true);
    await act(async () => {
      fireEvent.click(autoCheckbox);
    });
    await waitFor(() => {
      expect(useAppStore.getState().autostartEnabled).toBe(false);
    });
  });
});
