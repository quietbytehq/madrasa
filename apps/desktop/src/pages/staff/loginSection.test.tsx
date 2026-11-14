import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { ApiError } from "@/api/client";
import { SessionProvider } from "@/app/session";
import type { Staff, UserSummary } from "@/api/types";
import { LoginSection } from "./StaffPage";

vi.mock("@/api/endpoints", () => ({
  api: {
    createStaffLogin: vi.fn(),
    updateStaffLogin: vi.fn(),
    resetStaffLoginPassword: vi.fn(),
  },
}));

const admin: UserSummary = {
  id: "u-admin",
  school_id: "s",
  username: "ayesha",
  full_name: "Ayesha Khan",
  email: null,
  is_active: true,
  roles: ["admin"],
};

const sana: Staff = {
  id: "st1",
  school_id: "s",
  user_id: null,
  staff_number: "T-1",
  full_name: "Sana Malik",
  father_name: "",
  gender: "female",
  phone: null,
  email: null,
  cnic: null,
  designation: "Teacher",
  joined_on: null,
  status: "active",
  custom: {},
  roles: [],
  username: null,
  login_active: null,
};

function render(staff: Staff, onChanged = vi.fn()) {
  renderWithProviders(
    <SessionProvider initial={{ user: admin, permissions: ["users.manage", "teachers.manage"] }}>
      <LoginSection staff={staff} onChanged={onChanged} />
    </SessionProvider>,
  );
  return onChanged;
}

beforeEach(() => vi.clearAllMocks());

describe("LoginSection", () => {
  it("asks before changing an existing login's role, and does nothing on no", async () => {
    vi.mocked(api.updateStaffLogin).mockResolvedValue({ ...sana, user_id: "u1" });
    render({ ...sana, user_id: "u1", username: "sana", roles: ["teacher"], login_active: true });
    await userEvent.selectOptions(screen.getByLabelText("Role"), "admin");
    const ask = await screen.findByTestId("change-role");
    expect(ask).toHaveTextContent("Change this login's role?");
    expect(ask).toHaveTextContent("Sana Malik already has a login: sana");
    expect(ask).toHaveTextContent("from Teacher to Administrator");
    expect(api.updateStaffLogin).not.toHaveBeenCalled();
    await userEvent.click(screen.getByTestId("change-role-no"));
    expect(api.updateStaffLogin).not.toHaveBeenCalled();
    expect(screen.queryByTestId("change-role")).not.toBeInTheDocument();

    await userEvent.selectOptions(screen.getByLabelText("Role"), "admin");
    await userEvent.click(await screen.findByTestId("change-role-yes"));
    await waitFor(() =>
      expect(api.updateStaffLogin).toHaveBeenCalledWith("st1", { role: "admin" }),
    );
  });

  it("shows the existing login when Create login finds one, and changes nothing unless asked", async () => {
    vi.mocked(api.createStaffLogin).mockRejectedValue(
      new ApiError(
        409,
        "login_exists",
        "Sana Malik already has a login: sana. Change this login's role or reset its password instead.",
        { username: "sana", role: "teacher" },
      ),
    );
    vi.mocked(api.updateStaffLogin).mockResolvedValue({ ...sana, user_id: "u1" });
    // The screen was opened before the login was made elsewhere: it still offers "Create login".
    const onChanged = render(sana);
    await userEvent.click(screen.getByRole("button", { name: "Create login" }));
    await userEvent.type(screen.getByLabelText("Username"), "sana2");
    await userEvent.type(screen.getByLabelText("Password"), "teacher-password-1");
    await userEvent.selectOptions(screen.getByLabelText("Role"), "accountant");
    await userEvent.click(screen.getByRole("button", { name: "Create login" }));
    const ask = await screen.findByTestId("change-role");
    expect(ask).toHaveTextContent("Change this login's role?");
    expect(ask).toHaveTextContent("already has a login: sana");
    expect(ask).toHaveTextContent("from Teacher to Accountant");
    expect(api.updateStaffLogin).not.toHaveBeenCalled();
    await userEvent.click(screen.getByTestId("change-role-no"));
    expect(api.updateStaffLogin).not.toHaveBeenCalled();
    expect(onChanged).toHaveBeenCalled();
  });
});
