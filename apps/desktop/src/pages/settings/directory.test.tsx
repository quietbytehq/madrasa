import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { FeaturesProvider } from "@/app/features";
import { SessionProvider } from "@/app/session";
import type { DirectorySettings, UserSummary } from "@/api/types";
import { DirectoryCard } from "./DirectoryCard";
import { Login } from "../Login";

vi.mock("@/api/endpoints", () => ({
  api: {
    directorySettings: vi.fn(),
    updateDirectorySettings: vi.fn(),
    testDirectory: vi.fn(),
    authOptions: vi.fn(),
    login: vi.fn(),
  },
}));

const admin: UserSummary = {
  id: "u1",
  school_id: "s",
  username: "admin",
  full_name: "Ayesha Khan",
  email: null,
  is_active: true,
  roles: ["admin"],
};

const settings: DirectorySettings = {
  enabled: false,
  url: "",
  bind_dn: "",
  user_base_dn: "",
  user_filter: "(&(objectClass=user)(sAMAccountName={username}))",
  username_attr: "sAMAccountName",
  name_attr: "displayName",
  email_attr: "mail",
  group_attr: "memberOf",
  group_roles: [],
  default_role: "",
  login_hint: "",
  has_bind_password: false,
  roles: ["principal", "admin", "accountant", "teacher", "librarian", "storekeeper"],
};

describe("DirectoryCard", () => {
  it("saves the settings with a group mapping and the bind password, then tests them", async () => {
    vi.mocked(api.directorySettings).mockResolvedValue(settings);
    vi.mocked(api.updateDirectorySettings).mockResolvedValue({
      ...settings,
      enabled: true,
      url: "ldaps://dc1.school.local",
      has_bind_password: true,
    });
    vi.mocked(api.testDirectory).mockResolvedValue({
      bind_ok: true,
      message: "Found Zara Qureshi; they would log in as teacher.",
      user: null,
      role: "teacher",
    });
    renderWithProviders(
      <FeaturesProvider
        initial={{ edition: "standard", features: ["dashboard", "settings", "directory_login"] }}
      >
        <SessionProvider initial={{ user: admin, permissions: ["settings.manage"] }}>
          <DirectoryCard />
        </SessionProvider>
      </FeaturesProvider>,
      { route: "/settings" },
    );
    expect(await screen.findByText("Off")).toBeInTheDocument();
    await userEvent.click(screen.getByLabelText("Use the school directory for logins"));
    await userEvent.type(screen.getByLabelText("Server URL"), "ldaps://dc1.school.local");
    await userEvent.type(screen.getByLabelText("Bind account DN"), "CN=madrasa,DC=school,DC=local");
    await userEvent.type(screen.getByLabelText("Bind account password"), "bind-secret");
    await userEvent.type(screen.getByLabelText("User base DN"), "OU=Staff,DC=school,DC=local");
    await userEvent.click(screen.getByTestId("add-mapping"));
    await userEvent.type(screen.getByLabelText("Group 1"), "CN=Teachers,DC=school,DC=local");
    await userEvent.selectOptions(screen.getByLabelText("Role 1"), "teacher");
    await userEvent.click(screen.getByTestId("save-directory"));
    await waitFor(() =>
      expect(api.updateDirectorySettings).toHaveBeenCalledWith(
        expect.objectContaining({
          enabled: true,
          url: "ldaps://dc1.school.local",
          bind_password: "bind-secret",
          group_roles: [{ group: "CN=Teachers,DC=school,DC=local", role: "teacher" }],
        }),
      ),
    );
    expect(await screen.findByText("On")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("Stored. Leave empty to keep it.")).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Try a username"), "zara");
    await userEvent.click(screen.getByTestId("test-directory"));
    await waitFor(() =>
      expect(api.testDirectory).toHaveBeenCalledWith(expect.objectContaining({ username: "zara" })),
    );
    expect(await screen.findByTestId("test-result")).toHaveTextContent("Zara Qureshi");
  });
});

describe("Login", () => {
  it("shows the directory hint when directory login is on", async () => {
    vi.mocked(api.authOptions).mockResolvedValue({
      directory_login: true,
      directory_hint: "Use your school network account.",
    });
    renderWithProviders(
      <FeaturesProvider initial={{ edition: "standard", features: ["dashboard"] }}>
        <SessionProvider initial="anonymous">
          <Login />
        </SessionProvider>
      </FeaturesProvider>,
      { route: "/login" },
    );
    const hint = await screen.findByTestId("directory-hint");
    expect(within(hint).getByText("Use your school network account.")).toBeInTheDocument();
  });
});
