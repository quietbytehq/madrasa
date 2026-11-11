import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import type { NexusLinkStatus, NexusOverwrite } from "@/api/types";
import { NexusCard, showValue } from "./NexusCard";

vi.mock("@/api/endpoints", () => ({
  api: {
    nexusLink: vi.fn(),
    nexusJoin: vi.fn(),
    nexusSync: vi.fn(),
    nexusLeave: vi.fn(),
    nexusOverwrites: vi.fn(),
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
  overwrites: 0,
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

beforeEach(() => {
  vi.resetAllMocks();
});

describe("NexusCard", () => {
  it("joins with the hub address and join code, then starts the first upload", async () => {
    vi.mocked(api.nexusLink).mockResolvedValue(unlinked);
    vi.mocked(api.nexusJoin).mockResolvedValue({ ...linked, last_sync_at: null, pending: 250 });
    vi.mocked(api.nexusSync).mockResolvedValue({ ...linked, pending: 0 });
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
    expect(screen.getByTestId("nexus-state")).toHaveTextContent("Joined");
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
    vi.mocked(api.nexusSync).mockResolvedValue({ ...linked, pending: 0, overwrites: 1 });
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
    await waitFor(() => expect(screen.getByTestId("nexus-state")).toHaveTextContent("Joined"));
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

  it("shows logged values readably", () => {
    expect(showValue(null)).toBe("—");
    expect(showValue("")).toBe("—");
    expect(showValue("absent")).toBe("absent");
    expect(showValue(12)).toBe("12");
    expect(showValue({ amount: "-500.00" })).toBe('{"amount":"-500.00"}');
    expect(showValue({ note: "x".repeat(200) }).length).toBe(78);
  });
});
