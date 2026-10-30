import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { SessionProvider } from "@/app/session";
import type { BackupSettings, BackupStatus, RestorePreview, UserSummary } from "@/api/types";
import { BackupPage } from "./BackupPage";

vi.mock("@/api/endpoints", () => ({
  api: {
    backupStatus: vi.fn(),
    backupSettings: vi.fn(),
    saveBackupSettings: vi.fn(),
    runBackup: vi.fn(),
    backupRuns: vi.fn(async () => []),
    backupFiles: vi.fn(),
    previewRestore: vi.fn(),
    restoreBackup: vi.fn(),
    changeBackupPassword: vi.fn(),
    unlockBackups: vi.fn(),
    logout: vi.fn(async () => {}),
  },
}));

const user: UserSummary = {
  id: "u1",
  school_id: "s",
  username: "ayesha",
  full_name: "Ayesha Khan",
  email: null,
  is_active: true,
  roles: ["admin"],
};
const status: BackupStatus = {
  last_backup_at: "2026-10-30T02:00:00Z",
  last_backup_file: "/home/u/Documents/Madrasa Backups/madrasa-khi-mc-20261030-020000.mbk",
  stale: false,
  enabled: true,
  time: "02:00",
  directory: "/home/u/Documents/Madrasa Backups",
  unattended_ready: true,
  last_error: null,
};
const settings: BackupSettings = { enabled: true, time: "02:00", directory: null, keep: 30 };
const preview: RestorePreview = {
  path: status.last_backup_file!,
  size: 2048,
  meta: {
    created_at: "2026-10-30T02:00:00Z",
    app_version: "0.1.0",
    school_id: "s",
    school_name: "Gulshan High",
    school_code: "KHI-MC",
    migrations: [1, 2, 3, 4, 5, 6],
    counts: { students: 1200, staff: 120, audit_log: 50 },
  },
  schema_matches: true,
  current_counts: { students: 1201, staff: 120, audit_log: 60 },
};

function render(permissions = ["backup.manage", "school.read"]) {
  return renderWithProviders(
    <SessionProvider initial={{ user, permissions }}>
      <BackupPage />
    </SessionProvider>,
  );
}

beforeEach(() => {
  vi.mocked(api.backupStatus).mockResolvedValue(status);
  vi.mocked(api.backupSettings).mockResolvedValue(settings);
  vi.mocked(api.backupFiles).mockResolvedValue([
    {
      path: status.last_backup_file!,
      name: "madrasa-khi-mc-20261030-020000.mbk",
      size: 2048,
      modified: "2026-10-30T02:00:00Z",
    },
  ]);
  vi.mocked(api.previewRestore).mockResolvedValue(preview);
  vi.mocked(api.restoreBackup).mockClear();
  vi.mocked(api.runBackup).mockClear();
});

describe("BackupPage", () => {
  it("shows the status and runs a backup now", async () => {
    vi.mocked(api.runBackup).mockResolvedValue({
      id: "r1",
      school_id: "s",
      trigger: "manual",
      status: "ok",
      started_at: "2026-10-30T10:00:00Z",
      finished_at: "2026-10-30T10:00:05Z",
      file_path: "/x/madrasa-khi-mc-20261030-100000.mbk",
      file_size: 4096,
      error: null,
    });
    render();
    expect(await screen.findByText("Backups are up to date.")).toBeInTheDocument();
    expect(screen.getByText("Automatic backup every day at 02:00")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Back up now" }));
    await waitFor(() => expect(api.runBackup).toHaveBeenCalled());
    expect(
      await screen.findByText("Backup written to /x/madrasa-khi-mc-20261030-100000.mbk."),
    ).toBeInTheDocument();
  });

  it("warns when stale and asks for the password once when unattended backups are not ready", async () => {
    vi.mocked(api.backupStatus).mockResolvedValue({
      ...status,
      stale: true,
      last_backup_at: null,
      unattended_ready: false,
      last_error: "disk full",
    });
    vi.mocked(api.unlockBackups).mockResolvedValue(undefined);
    render();
    expect(await screen.findByText("The last backup is older than 2 days.")).toBeInTheDocument();
    expect(screen.getByText("No backup yet")).toBeInTheDocument();
    expect(screen.getByText("Last attempt failed: disk full")).toBeInTheDocument();
    await userEvent.type(screen.getAllByLabelText("Backup password")[0]!, "backup-password-1");
    await userEvent.click(screen.getByRole("button", { name: "Enable automatic backups" }));
    await waitFor(() =>
      expect(api.unlockBackups).toHaveBeenCalledWith({ password: "backup-password-1" }),
    );
  });

  it("previews a file, then restores with the password and logs out", async () => {
    vi.mocked(api.restoreBackup).mockResolvedValue({
      tables: 36,
      rows: 5000,
      meta: preview.meta,
      relogin: true,
    });
    render();
    await userEvent.click(await screen.findByRole("button", { name: "Restore" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Gulshan High · KHI-MC")).toBeInTheDocument();
    const students = within(dialog).getByText("Students").closest("tr")!;
    expect(students).toHaveTextContent("1200");
    expect(students).toHaveTextContent("1201");
    expect(within(dialog).getByRole("alert")).toHaveTextContent("replaces everything");
    const confirm = within(dialog).getByRole("button", { name: "Restore now" });
    expect(confirm).toBeDisabled();
    await userEvent.type(within(dialog).getByLabelText("Backup password"), "backup-password-1");
    await userEvent.click(confirm);
    await waitFor(() =>
      expect(api.restoreBackup).toHaveBeenCalledWith({
        path: preview.path,
        password: "backup-password-1",
        recovery_key: undefined,
      }),
    );
    expect(await screen.findByText(/Restored 5000 records/)).toBeInTheDocument();
    expect(api.logout).toHaveBeenCalled();
  });

  it("refuses to offer a restore when the schema differs and supports the recovery key", async () => {
    vi.mocked(api.previewRestore).mockResolvedValue({ ...preview, schema_matches: false });
    render();
    await userEvent.click(await screen.findByRole("button", { name: "Restore" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getAllByRole("alert")[1]).toHaveTextContent("different version");
    expect(within(dialog).queryByRole("button", { name: "Restore now" })).not.toBeInTheDocument();
  });

  it("changes the backup password with a warning about old backups", async () => {
    vi.mocked(api.changeBackupPassword).mockResolvedValue(undefined);
    render();
    const note = await screen.findByRole("note");
    expect(note).toHaveTextContent("applies to new backups only");
    await userEvent.type(screen.getByLabelText("Current backup password"), "backup-password-1");
    await userEvent.type(screen.getByLabelText("New backup password"), "new-backup-pw-1");
    await userEvent.type(screen.getByLabelText("Confirm new password"), "new-backup-pw-2");
    await userEvent.click(screen.getByRole("button", { name: "Change password" }));
    expect(screen.getByText("The new passwords do not match.")).toBeInTheDocument();
    expect(api.changeBackupPassword).not.toHaveBeenCalled();
    await userEvent.clear(screen.getByLabelText("Confirm new password"));
    await userEvent.type(screen.getByLabelText("Confirm new password"), "new-backup-pw-1");
    await userEvent.click(screen.getByRole("button", { name: "Change password" }));
    await waitFor(() =>
      expect(api.changeBackupPassword).toHaveBeenCalledWith({
        current_password: "backup-password-1",
        new_password: "new-backup-pw-1",
      }),
    );
    expect(await screen.findByText(/Backup password changed/)).toBeInTheDocument();
  });

  it("saves the schedule settings", async () => {
    vi.mocked(api.saveBackupSettings).mockImplementation(async (s) => s);
    render();
    const time = await screen.findByLabelText("Time");
    await userEvent.clear(time);
    await userEvent.type(time, "23:30");
    await userEvent.type(screen.getByLabelText(/Backup folder/), "E:\\Madrasa");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(api.saveBackupSettings).toHaveBeenCalledWith({
        enabled: true,
        time: "23:30",
        directory: "E:\\Madrasa",
        keep: 30,
      }),
    );
  });
});
