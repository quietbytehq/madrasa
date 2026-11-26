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
    exportEverything: vi.fn(async () => {}),
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
  never: false,
  stale: false,
  enabled: true,
  time: "02:00",
  directory: "/home/u/Documents/Madrasa Backups",
  unattended_ready: true,
  last_error: null,
  last_failed_at: null,
  check: null,
  check_message: null,
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
  vi.mocked(api.changeBackupPassword).mockClear();
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
      verify_status: null,
      verify_message: null,
      verified_at: null,
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

  it("exports everything as a workbook in one click, and says what it is", async () => {
    vi.mocked(api.backupStatus).mockResolvedValue(status);
    vi.mocked(api.backupSettings).mockResolvedValue(settings);
    vi.mocked(api.backupFiles).mockResolvedValue([]);
    render();
    const button = await screen.findByRole("button", { name: "Export everything" });
    expect(button).toHaveAttribute("title", expect.stringContaining("without passwords or keys"));
    await userEvent.click(button);
    expect(api.exportEverything).toHaveBeenCalledTimes(1);
    expect(await screen.findByText(/It holds every record of the school/)).toBeInTheDocument();
  });

  it("after Back up now it says the check is running, and then its result", async () => {
    vi.mocked(api.backupSettings).mockResolvedValue(settings);
    vi.mocked(api.backupFiles).mockResolvedValue([]);
    vi.mocked(api.runBackup).mockResolvedValue({
      id: "r2",
      school_id: "s",
      trigger: "manual",
      status: "ok",
      started_at: "2026-10-30T10:00:00Z",
      finished_at: "2026-10-30T10:00:05Z",
      file_path: "/x/new.mbk",
      file_size: 4096,
      error: null,
      verify_status: null,
      verify_message: null,
      verified_at: null,
    });
    // The backup before this one was checked; the new one is not yet, then it is.
    vi.mocked(api.backupStatus)
      .mockResolvedValueOnce({ ...status, check: "verified", check_message: "old" })
      .mockResolvedValueOnce({ ...status, check: null })
      .mockResolvedValue({
        ...status,
        check: "verified",
        check_message:
          "Restored into a temporary database and counted: all 5120 records are there.",
      });
    render();
    expect(await screen.findByTestId("backup-check")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Back up now" }));
    expect(await screen.findByTestId("backup-checking")).toHaveTextContent(
      "Checking the backup: it is being restored into a temporary database and counted…",
    );
    expect(screen.queryByTestId("backup-check")).not.toBeInTheDocument();
    const done = await screen.findByTestId("backup-check", {}, { timeout: 6000 });
    expect(done).toHaveAttribute("title", expect.stringContaining("all 5120 records"));
    expect(screen.queryByTestId("backup-checking")).not.toBeInTheDocument();
  }, 10_000);

  it("says when the last backup was checked, and alerts when it did not pass", async () => {
    vi.mocked(api.backupSettings).mockResolvedValue(settings);
    vi.mocked(api.backupFiles).mockResolvedValue([]);
    vi.mocked(api.backupStatus).mockResolvedValue({
      ...status,
      check: "verified",
      check_message: "Restored into a temporary database and counted: all 5120 records are there.",
    });
    const run = {
      id: "r1",
      school_id: "s",
      trigger: "scheduled" as const,
      status: "ok" as const,
      started_at: "2026-10-30T02:00:00Z",
      finished_at: "2026-10-30T02:00:05Z",
      file_path: "/x/a.mbk",
      file_size: 4096,
      error: null,
      verify_status: "verified" as const,
      verify_message: "Restored into a temporary database and counted: all 5120 records are there.",
      verified_at: "2026-10-30T02:00:09Z",
    };
    vi.mocked(api.backupRuns).mockResolvedValueOnce([
      run,
      {
        ...run,
        id: "r0",
        file_path: "/x/b.mbk",
        verify_status: "failed",
        verify_message: "students: 10 of 12",
      },
    ]);
    const first = render();
    // The history says for each backup whether it was checked.
    expect(await screen.findByText("Checked")).toHaveAttribute(
      "title",
      expect.stringContaining("all 5120 records"),
    );
    expect(screen.getByText("Check failed")).toHaveAttribute("title", "students: 10 of 12");
    const line = await screen.findByTestId("backup-check");
    expect(line).toHaveTextContent("Checked after writing: restored into a temporary database");
    expect(line).toHaveAttribute("title", expect.stringContaining("all 5120 records"));
    expect(screen.queryByTestId("backup-alert")).not.toBeInTheDocument();
    first.unmount();

    vi.mocked(api.backupStatus).mockResolvedValue({
      ...status,
      check: "failed",
      check_message:
        "Restored into a temporary database, but some records are missing (students: 10 of 12).",
    });
    render();
    const alert = within(await screen.findByTestId("backup-alert"));
    expect(alert.getByText("The last backup did not pass its check")).toBeInTheDocument();
    expect(alert.getByText(/students: 10 of 12/)).toBeInTheDocument();
    expect(alert.getByText(/Do not rely on it/)).toBeInTheDocument();
    expect(screen.queryByTestId("backup-check")).not.toBeInTheDocument();
  });

  it("with no good backup at all, the alert says nothing is backed up", async () => {
    vi.mocked(api.backupSettings).mockResolvedValue(settings);
    vi.mocked(api.backupFiles).mockResolvedValue([]);
    vi.mocked(api.backupStatus).mockResolvedValue({
      ...status,
      never: true,
      last_backup_at: null,
      last_backup_file: null,
      last_error: "The backup folder cannot be used (E:\\Backups): not found",
      last_failed_at: "2026-11-25T02:00:05Z",
    });
    render();
    const alert = within(await screen.findByTestId("backup-alert"));
    expect(alert.getByText(/The backup folder cannot be used/)).toBeInTheDocument();
    expect(alert.getByText("There is no good backup yet: nothing is backed up.")).toBeVisible();
  });

  it("on a new install says there is no backup yet, without the two-day warning", async () => {
    vi.mocked(api.backupStatus).mockResolvedValue({
      ...status,
      never: true,
      stale: false,
      last_backup_at: null,
      last_backup_file: null,
    });
    render();
    expect(await screen.findByText("No backup yet — run one now")).toBeInTheDocument();
    expect(screen.queryByText("The last backup is older than 2 days.")).toBeNull();
    expect(screen.queryByText("Backups are up to date.")).toBeNull();
  });

  it("warns when stale and asks for the password once when unattended backups are not ready", async () => {
    vi.mocked(api.backupStatus).mockResolvedValue({
      ...status,
      stale: true,
      last_backup_at: "2026-10-20T02:00:00Z",
      unattended_ready: false,
      last_error: "disk full",
    });
    vi.mocked(api.unlockBackups).mockResolvedValue(undefined);
    render();
    expect(await screen.findByText("The last backup is older than 2 days.")).toBeInTheDocument();
    // The failure is said in an alert of its own, with the last good backup and what to do.
    const alert = within(screen.getByTestId("backup-alert"));
    expect(alert.getByText("The last backup failed")).toBeInTheDocument();
    expect(alert.getByText("disk full")).toBeInTheDocument();
    expect(alert.getByText(/The last good backup is from .*Nothing entered since/)).toBeVisible();
    expect(alert.getByText(/then press Back up now/)).toBeInTheDocument();
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
    // Not joined to a school network: no network warning, nothing more to confirm.
    expect(within(dialog).queryByTestId("restore-network")).not.toBeInTheDocument();
    expect(within(dialog).queryByTestId("restore-network-confirm")).not.toBeInTheDocument();
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

  it("warns, and asks for a confirmation, when the school is joined to a network", async () => {
    vi.mocked(api.previewRestore).mockResolvedValue({
      ...preview,
      network: "Gulshan Education Network",
    });
    vi.mocked(api.restoreBackup).mockResolvedValue({
      tables: 36,
      rows: 5000,
      meta: preview.meta,
      relogin: true,
      network: "Gulshan Education Network",
    });
    render();
    await userEvent.click(await screen.findByRole("button", { name: "Restore" }));
    const dialog = await screen.findByRole("dialog");
    const warning = within(dialog).getByTestId("restore-network");
    expect(warning).toHaveTextContent(
      "This school is joined to a network. Restoring an older backup will make your data differ from head office's copy. Madrasa will ask head office to reset its copy.",
    );
    expect(warning).toHaveTextContent("Gulshan Education Network");
    await userEvent.type(within(dialog).getByLabelText("Backup password"), "backup-password-1");
    const confirm = within(dialog).getByRole("button", { name: "Restore now" });
    expect(confirm).toBeDisabled();
    await userEvent.click(within(dialog).getByTestId("restore-network-confirm"));
    expect(confirm).toBeEnabled();
    await userEvent.click(confirm);
    await waitFor(() =>
      expect(api.restoreBackup).toHaveBeenCalledWith({
        path: preview.path,
        password: "backup-password-1",
        recovery_key: undefined,
        released_key: undefined,
        confirm_network: true,
      }),
    );
    expect(
      await screen.findByText(
        /Head office \(Gulshan Education Network\) will be asked to reset its copy/,
      ),
    ).toBeInTheDocument();
  });

  it("refuses to offer a restore when the schema differs and supports the recovery key", async () => {
    vi.mocked(api.previewRestore).mockResolvedValue({ ...preview, schema_matches: false });
    render();
    await userEvent.click(await screen.findByRole("button", { name: "Restore" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getAllByRole("alert")[1]).toHaveTextContent("different version");
    expect(within(dialog).queryByRole("button", { name: "Restore now" })).not.toBeInTheDocument();
  });

  it("restores with the recovery key or a key released by head office", async () => {
    vi.mocked(api.restoreBackup).mockResolvedValue({
      tables: 36,
      rows: 5000,
      meta: preview.meta,
      relogin: true,
    });
    render();
    await userEvent.click(await screen.findByRole("button", { name: "Restore" }));
    const dialog = await screen.findByRole("dialog");
    // Three ways in; the two not showing are offered as links.
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Use the recovery key instead" }),
    );
    expect(within(dialog).getByLabelText("Recovery key")).toBeInTheDocument();
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Use a key released by head office instead" }),
    );
    const code = "0123A-BCDEF-GHJKM-NPQRS-TVWXY-Z0123-45678-9ABCD-EFGHJ-KMNPQ-RSTVW";
    const field = within(dialog).getByLabelText("Key released by head office");
    expect(field).toHaveAttribute("type", "text");
    expect(within(dialog).getByText(/eleven groups of five characters/)).toBeInTheDocument();
    await userEvent.type(field, code);
    await userEvent.click(within(dialog).getByRole("button", { name: "Restore now" }));
    await waitFor(() =>
      expect(api.restoreBackup).toHaveBeenCalledWith({
        path: preview.path,
        password: undefined,
        recovery_key: undefined,
        released_key: code,
      }),
    );
  });

  it("sets a new backup password with a key released by head office", async () => {
    vi.mocked(api.changeBackupPassword).mockResolvedValue(undefined);
    render();
    await userEvent.click(
      await screen.findByRole("button", { name: "I have a key released by head office instead" }),
    );
    await userEvent.type(screen.getByLabelText("Key released by head office"), "0123A-BCDEF");
    await userEvent.type(screen.getByLabelText("New backup password"), "new-backup-pw-1");
    await userEvent.type(screen.getByLabelText("Confirm new password"), "new-backup-pw-1");
    await userEvent.click(screen.getByRole("button", { name: "Change password" }));
    await waitFor(() =>
      expect(api.changeBackupPassword).toHaveBeenCalledWith({
        released_key: "0123A-BCDEF",
        new_password: "new-backup-pw-1",
      }),
    );
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
