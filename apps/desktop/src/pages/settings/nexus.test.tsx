import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import type { NexusEscrowView, NexusLinkStatus, NexusOverwrite } from "@/api/types";
import { showValue } from "@/lib/showValue";
import { NexusCard } from "./NexusCard";

vi.mock("@/api/endpoints", () => ({
  api: {
    nexusLink: vi.fn(),
    nexusJoin: vi.fn(),
    nexusSync: vi.fn(),
    nexusLeave: vi.fn(),
    nexusOverwrites: vi.fn(),
    nexusEscrow: vi.fn(),
    nexusEscrowDeposit: vi.fn(),
    nexusEscrowWithdraw: vi.fn(),
  },
}));

const unlinked: NexusLinkStatus = {
  linked: false,
  hub_url: null,
  hub_name: null,
  joined_at: null,
  last_sync_at: null,
  last_error: null,
  last_report: null,
  ended: null,
  pending: 0,
  parked: [],
  parked_at_hub: [],
  overwrites: 0,
  restored: null,
  in_step: false,
};

const linked: NexusLinkStatus = {
  ...unlinked,
  linked: true,
  hub_url: "https://nexus.example.org",
  hub_name: "Gulshan Education Network",
  joined_at: "2026-11-01T09:00:00Z",
  last_sync_at: "2026-11-11T10:00:00Z",
  pending: 3,
};

const overwrite: NexusOverwrite = {
  uid: "o1",
  at: "2026-11-11T10:00:00Z",
  table: "students",
  id: "s1",
  field: "full_name",
  kept: "Sara Noor (head office)",
  discarded: "Sara Noor (school)",
  kept_origin: "head_office",
  reason: "head_office_wins",
};

const noEscrow: NexusEscrowView = {
  available: false,
  deposited: false,
  current: false,
  deposited_at: null,
  releases: [],
};

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.nexusEscrow).mockResolvedValue(noEscrow);
});

describe("NexusCard", () => {
  it("joins with the hub address and join code, then starts the first upload", async () => {
    vi.mocked(api.nexusLink).mockResolvedValue(unlinked);
    vi.mocked(api.nexusJoin).mockResolvedValue({ ...linked, last_sync_at: null, pending: 250 });
    vi.mocked(api.nexusSync).mockResolvedValue({ ...linked, pending: 0, in_step: true });
    renderWithProviders(<NexusCard />);
    const join = await screen.findByRole("button", { name: "Join the network" });
    expect(join).toBeDisabled();
    // What joining means is said before the button, not after.
    expect(screen.getByText(/head office can read this school's records/)).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Hub address"), " https://nexus.example.org ");
    await userEvent.type(
      screen.getByLabelText("Join code"),
      "ABCD-EFGH-JKMN-PQRS-TVWX-YZ12-3456-789A",
    );
    await userEvent.click(join);
    await waitFor(() =>
      expect(api.nexusJoin).toHaveBeenCalledWith(
        "https://nexus.example.org",
        "ABCD-EFGH-JKMN-PQRS-TVWX-YZ12-3456-789A",
      ),
    );
    await waitFor(() => expect(api.nexusSync).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("Gulshan Education Network")).toBeInTheDocument();
    expect(screen.getByTestId("nexus-state")).toHaveTextContent("In step");
    expect(screen.getByTestId("nexus-pending")).toHaveTextContent("0");
  });

  it("shows a refused join in the hub's own words and stays on the form", async () => {
    vi.mocked(api.nexusLink).mockResolvedValue(unlinked);
    vi.mocked(api.nexusJoin).mockRejectedValue(
      Object.assign(new Error("This join code is not valid."), { name: "ApiError" }),
    );
    renderWithProviders(<NexusCard />);
    await userEvent.type(await screen.findByLabelText("Hub address"), "https://nexus.example.org");
    await userEvent.type(screen.getByLabelText("Join code"), "WRONG");
    await userEvent.click(screen.getByRole("button", { name: "Join the network" }));
    await waitFor(() => expect(api.nexusJoin).toHaveBeenCalled());
    expect(api.nexusSync).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Join the network" })).toBeInTheDocument();
  });

  it("shows the state of a joined school, problems, waiting records and the overwrite log", async () => {
    vi.mocked(api.nexusLink).mockResolvedValue({
      ...linked,
      last_error: "The hub could not be reached.",
      parked: [
        { table: "students", id: "x", reason: "Another record here already uses the same value." },
      ],
      overwrites: 1,
    });
    vi.mocked(api.nexusOverwrites).mockResolvedValue([overwrite]);
    vi.mocked(api.nexusSync).mockResolvedValue({
      ...linked,
      pending: 0,
      overwrites: 1,
      in_step: true,
    });
    renderWithProviders(<NexusCard />);
    expect(await screen.findByTestId("nexus-state")).toHaveTextContent("Needs attention");
    expect(screen.getByRole("alert")).toHaveTextContent("The hub could not be reached.");
    expect(
      screen.getByText(/1 record\(s\) from head office could not be stored here yet/),
    ).toBeInTheDocument();
    expect(screen.getByText(/Another record here already uses the same value/)).toBeInTheDocument();
    expect(screen.getByTestId("nexus-pending")).toHaveTextContent("3");
    // The log is fetched when asked for.
    expect(api.nexusOverwrites).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Show" }));
    const row = (await screen.findByText("Sara Noor (school)")).closest("tr")!;
    expect(within(row).getByText("Sara Noor (head office)")).toBeInTheDocument();
    expect(within(row).getByText("students.full_name")).toBeInTheDocument();
    expect(within(row).getByText("Head office changed it too")).toBeInTheDocument();
    // Sync now refreshes the state.
    await userEvent.click(screen.getByRole("button", { name: "Sync now" }));
    await waitFor(() => expect(api.nexusSync).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByTestId("nexus-state")).toHaveTextContent("In step"));
    expect(screen.getByTestId("nexus-pending")).toHaveTextContent("0");
  });

  it("asks before leaving and says when the hub could not be told", async () => {
    vi.mocked(api.nexusLink).mockResolvedValue(linked);
    vi.mocked(api.nexusLeave).mockResolvedValue({
      hub_told: false,
      status: {
        ...unlinked,
        hub_name: "Gulshan Education Network",
        ended: { at: "2026-11-11T11:00:00Z", by: "school", reason: "Moving" },
      },
    });
    renderWithProviders(<NexusCard />);
    await userEvent.click(await screen.findByRole("button", { name: "Leave the network" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("keeps every record it has");
    expect(api.nexusLeave).not.toHaveBeenCalled();
    await userEvent.type(within(dialog).getByLabelText(/Reason/), "Moving");
    await userEvent.click(within(dialog).getByRole("button", { name: "Leave" }));
    await waitFor(() => expect(api.nexusLeave).toHaveBeenCalledWith("Moving"));
    expect(await screen.findByText(/the hub could not be reached to be told/)).toBeInTheDocument();
    // Back on the join form, with a note of what happened.
    expect(await screen.findByRole("button", { name: "Join the network" })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(
      "This school left Gulshan Education Network",
    );
  });

  it("tells a school that head office removed it, with the reason", async () => {
    vi.mocked(api.nexusLink).mockResolvedValue({
      ...unlinked,
      hub_name: "Gulshan Education Network",
      ended: {
        at: "2026-11-11T11:00:00Z",
        by: "head_office",
        reason: "Head office removed this school from the network: The school closed.",
      },
    });
    renderWithProviders(<NexusCard />);
    await screen.findByRole("button", { name: "Join the network" });
    const note = screen.getByRole("status");
    expect(note).toHaveTextContent(
      "Head office removed this school from the network: The school closed.",
    );
    expect(note).toHaveTextContent("All records are still here.");
  });

  it("leaves the backup key with head office and takes it back", async () => {
    vi.mocked(api.nexusLink).mockResolvedValue(linked);
    vi.mocked(api.nexusEscrow).mockResolvedValue({ ...noEscrow, available: true });
    const held = {
      ...noEscrow,
      available: true,
      deposited: true,
      current: true,
      deposited_at: "2026-11-11T10:00:00Z",
    };
    vi.mocked(api.nexusEscrowDeposit).mockResolvedValue(held);
    vi.mocked(api.nexusEscrowWithdraw).mockResolvedValue({ ...noEscrow, available: true });
    renderWithProviders(<NexusCard canManageBackups />);
    // What leaving the key means is said before the button.
    expect(await screen.findByText(/This is your choice; syncing works without it/)).toBeVisible();
    const leave = await screen.findByRole("button", { name: "Leave the key with head office" });
    expect(leave).toBeDisabled();
    await userEvent.type(screen.getByLabelText("Backup password"), "backup-password-1");
    await userEvent.click(leave);
    await waitFor(() => expect(api.nexusEscrowDeposit).toHaveBeenCalledWith("backup-password-1"));
    expect(await screen.findByTestId("nexus-escrow-state")).toHaveTextContent(
      "has been with head office since",
    );
    expect(screen.queryByLabelText("Backup password")).not.toBeInTheDocument();
    // Taking it back asks first and says what is at stake.
    await userEvent.click(screen.getByRole("button", { name: "Take the key back" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("nobody can open the backups");
    expect(api.nexusEscrowWithdraw).not.toHaveBeenCalled();
    await userEvent.click(within(dialog).getByRole("button", { name: "Take the key back" }));
    await waitFor(() => expect(api.nexusEscrowWithdraw).toHaveBeenCalled());
    expect(
      await screen.findByRole("button", { name: "Leave the key with head office" }),
    ).toBeInTheDocument();
  });

  it("shows every release of the key, and a key that is out of date", async () => {
    vi.mocked(api.nexusLink).mockResolvedValue(linked);
    vi.mocked(api.nexusEscrow).mockResolvedValue({
      available: true,
      deposited: true,
      current: false,
      deposited_at: "2026-11-01T10:00:00Z",
      releases: [
        {
          id: "r1",
          released_at: "2026-11-10T09:30:00Z",
          released_by: "Head Office",
          reason: "Principal lost the backup password",
          key_id: "k",
          seen: true,
        },
      ],
    });
    renderWithProviders(<NexusCard canManageBackups />);
    const alerts = await screen.findAllByRole("alert");
    expect(alerts[0]).toHaveTextContent("Head office has opened this school's backup key");
    expect(alerts[0]).toHaveTextContent("by Head Office: Principal lost the backup password");
    expect(alerts[0]).toHaveTextContent("If you did not ask for it, speak to head office.");
    expect(alerts[1]).toHaveTextContent("not this school's current backup key");
    // The key can be left again.
    expect(screen.getByRole("button", { name: "Leave the key with head office" })).toBeDisabled();
  });

  it("says so when head office has no escrow, cannot be reached, or the user may not hand the key over", async () => {
    vi.mocked(api.nexusLink).mockResolvedValue(linked);
    const { unmount } = renderWithProviders(<NexusCard canManageBackups />);
    expect(await screen.findByTestId("nexus-escrow-state")).toHaveTextContent(
      "Head office has not set up key escrow yet.",
    );
    expect(screen.queryByLabelText("Backup password")).not.toBeInTheDocument();
    unmount();

    vi.mocked(api.nexusEscrow).mockResolvedValue({ ...noEscrow, available: true });
    const second = renderWithProviders(<NexusCard />);
    expect(await screen.findByTestId("nexus-escrow-state")).toHaveTextContent(
      "Someone who manages backups can leave the backup key with head office.",
    );
    expect(screen.queryByLabelText("Backup password")).not.toBeInTheDocument();
    second.unmount();

    vi.mocked(api.nexusEscrow).mockRejectedValueOnce(new Error("network"));
    vi.mocked(api.nexusEscrow).mockResolvedValue({ ...noEscrow, available: true });
    renderWithProviders(<NexusCard canManageBackups />);
    expect(
      await screen.findByText(/Head office could not be reached to check the backup key/),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(
      await screen.findByRole("button", { name: "Leave the key with head office" }),
    ).toBeInTheDocument();
  });

  it("shows logged values readably", () => {
    expect(showValue(null)).toBe("—");
    expect(showValue("")).toBe("—");
    expect(showValue("absent")).toBe("absent");
    expect(showValue(12)).toBe("12");
    expect(showValue({ amount: "-500.00" })).toBe('{"amount":"-500.00"}');
    expect(showValue({ note: "x".repeat(200) }).length).toBe(78);
  });
  it("says in step only when nothing waits on either side, and counts records parked at head office", async () => {
    vi.mocked(api.nexusLink).mockResolvedValue({ ...linked, pending: 0, in_step: true });
    const { unmount } = renderWithProviders(<NexusCard />);
    expect(await screen.findByTestId("nexus-in-step")).toHaveTextContent(
      "In step with head office",
    );
    expect(screen.getByTestId("nexus-state")).toHaveTextContent("In step");
    unmount();

    vi.mocked(api.nexusLink).mockResolvedValue({
      ...linked,
      pending: 0,
      in_step: false,
      parked_at_hub: [
        { table: "students", id: "s9", reason: "Another student already has number 2026-0003." },
      ],
    });
    vi.mocked(api.nexusSync).mockResolvedValue({
      ...linked,
      pending: 0,
      in_step: false,
      parked_at_hub: [
        { table: "students", id: "s9", reason: "Another student already has number 2026-0003." },
      ],
    });
    renderWithProviders(<NexusCard />);
    expect(await screen.findByTestId("nexus-parked-at-hub")).toHaveTextContent("1");
    const detail = screen.getByTestId("nexus-parked-at-hub-detail");
    expect(detail).toHaveTextContent("1 record(s) from this school are waiting at head office");
    expect(detail).toHaveTextContent("Another student already has number 2026-0003.");
    expect(screen.getByTestId("nexus-in-step")).not.toHaveTextContent("In step with head office");
    expect(screen.getByTestId("nexus-state")).toHaveTextContent("Not in step yet");
    await userEvent.click(screen.getByRole("button", { name: "Sync now" }));
    expect(await screen.findByText(/not everything is in step/)).toBeInTheDocument();
    expect(screen.queryByText(/nothing is waiting on either side/)).not.toBeInTheDocument();
  });

  it("says when head office will be, and has been, asked to reset its copy after a restore", async () => {
    vi.mocked(api.nexusLink).mockResolvedValue({
      ...linked,
      in_step: false,
      restored: {
        at: "2026-11-13T08:00:00Z",
        backup_created_at: "2026-11-01T02:00:00Z",
        hub_reset_at: null,
        archive: null,
      },
    });
    const { unmount } = renderWithProviders(<NexusCard />);
    expect(await screen.findByTestId("nexus-restored")).toHaveTextContent(
      "At the next sync head office will be asked to set its copy aside",
    );
    unmount();
    vi.mocked(api.nexusLink).mockResolvedValue({
      ...linked,
      pending: 0,
      in_step: true,
      restored: {
        at: "2026-11-13T08:00:00Z",
        backup_created_at: "2026-11-01T02:00:00Z",
        hub_reset_at: "2026-11-13T08:05:00Z",
        archive: "a_0123",
      },
    });
    renderWithProviders(<NexusCard />);
    expect(await screen.findByTestId("nexus-restored")).toHaveTextContent(
      "head office set its old copy aside",
    );
  });
});
