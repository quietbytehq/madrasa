import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { SessionProvider } from "@/app/session";
import type { Staff, UserAccount, UserSummary } from "@/api/types";
import { UsersCard } from "./UsersCard";

vi.mock("@/api/endpoints", () => ({
  api: {
    users: vi.fn(),
    staff: vi.fn(),
    createUser: vi.fn(),
    updateUser: vi.fn(),
    resetUserPassword: vi.fn(),
  },
}));

const me: UserSummary = {
  id: "u-admin",
  school_id: "s",
  username: "ayesha",
  full_name: "Ayesha Khan",
  email: null,
  is_active: true,
  roles: ["admin", "principal"],
};
const accounts: UserAccount[] = [
  { ...me, staff_id: null, staff_name: null, last_login_at: "2026-10-31T05:00:00Z" },
  {
    id: "u-sana",
    school_id: "s",
    username: "sana",
    full_name: "Sana Malik",
    email: null,
    is_active: true,
    roles: ["teacher"],
    staff_id: "st-1",
    staff_name: "Sana Malik",
    last_login_at: null,
  },
];
const staff = [
  {
    id: "st-2",
    staff_number: "S-002",
    full_name: "Tariq Siddiqui",
    user_id: null,
    status: "active",
  },
  {
    id: "st-1",
    staff_number: "S-001",
    full_name: "Sana Malik",
    user_id: "u-sana",
    status: "active",
  },
] as unknown as Staff[];

function render() {
  return renderWithProviders(
    <SessionProvider initial={{ user: me, permissions: ["users.manage"] }}>
      <UsersCard />
    </SessionProvider>,
  );
}

beforeEach(() => {
  vi.mocked(api.users).mockResolvedValue(accounts);
  vi.mocked(api.staff).mockResolvedValue(staff);
  vi.mocked(api.updateUser).mockClear();
  vi.mocked(api.updateUser).mockImplementation(async (id, body) => ({
    ...accounts.find((a) => a.id === id)!,
    ...(body.is_active !== undefined ? { is_active: body.is_active } : {}),
    ...(body.role ? { roles: [body.role] } : {}),
  }));
  vi.mocked(api.createUser).mockClear();
  vi.mocked(api.createUser).mockResolvedValue(accounts[1]!);
  vi.mocked(api.resetUserPassword).mockClear();
  vi.mocked(api.resetUserPassword).mockResolvedValue(accounts[1]!);
});

describe("UsersCard", () => {
  it("lists every login with role, staff link, last login and status", async () => {
    render();
    const sana = await screen.findByTestId("user-sana");
    // The name and the linked staff member are both Sana Malik.
    expect(within(sana).getAllByText("Sana Malik")).toHaveLength(2);
    expect(within(sana).getByLabelText("sana: Role")).toHaveValue("teacher");
    expect(within(sana).getByText("Never")).toBeInTheDocument();
    expect(within(sana).getByText("Active")).toBeInTheDocument();
    const admin = screen.getByTestId("user-ayesha");
    expect(within(admin).getByText("—")).toBeInTheDocument();
    // Nobody can disable their own account from here.
    expect(within(admin).getByLabelText("ayesha: Can log in")).toBeDisabled();
  });

  it("changes a role, disables a login and resets a password", async () => {
    render();
    const sana = await screen.findByTestId("user-sana");
    await userEvent.selectOptions(within(sana).getByLabelText("sana: Role"), "accountant");
    await waitFor(() =>
      expect(api.updateUser).toHaveBeenCalledWith("u-sana", { role: "accountant" }),
    );
    await userEvent.click(within(sana).getByLabelText("sana: Can log in"));
    await waitFor(() =>
      expect(api.updateUser).toHaveBeenCalledWith("u-sana", { is_active: false }),
    );
    await userEvent.click(within(sana).getByRole("button", { name: "Reset password" }));
    const dialog = await screen.findByRole("dialog", { name: "Reset password: sana" });
    await userEvent.type(within(dialog).getByLabelText("New password"), "new-password-22");
    await userEvent.click(within(dialog).getByRole("button", { name: "Reset password" }));
    await waitFor(() =>
      expect(api.resetUserPassword).toHaveBeenCalledWith("u-sana", "new-password-22"),
    );
  });

  it("creates a login for a staff member who has none", async () => {
    render();
    await screen.findByTestId("user-sana");
    await userEvent.click(screen.getByRole("button", { name: "Create login" }));
    const dialog = await screen.findByRole("dialog", { name: "Create login" });
    // Only staff without a login are offered.
    const picker = within(dialog).getByLabelText("Staff member");
    expect(
      within(picker)
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["Tariq Siddiqui (S-002)"]);
    await userEvent.type(within(dialog).getByLabelText("Username"), "tariq");
    await userEvent.type(within(dialog).getByLabelText("Password"), "tariq-password-1");
    await userEvent.selectOptions(within(dialog).getByLabelText("Role"), "accountant");
    await userEvent.click(within(dialog).getByRole("button", { name: "Create login" }));
    await waitFor(() =>
      expect(api.createUser).toHaveBeenCalledWith({
        staff_id: "st-2",
        username: "tariq",
        password: "tariq-password-1",
        role: "accountant",
      }),
    );
  });
});
