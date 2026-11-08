import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { FeaturesProvider } from "@/app/features";
import { SessionProvider } from "@/app/session";
import type { ApiKey, ApiKeysView, UserSummary } from "@/api/types";
import { ApiKeysCard } from "./ApiKeysCard";

vi.mock("@/api/endpoints", () => ({
  api: {
    apiKeys: vi.fn(),
    createApiKey: vi.fn(),
    revokeApiKey: vi.fn(),
    apiKeyCalls: vi.fn(),
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

const portal: ApiKey = {
  id: "k1",
  school_id: "s",
  name: "Portal",
  key_prefix: "mk_abcdefgh",
  scopes: ["students", "attendance"],
  ip_allow: ["10.0.0.0/8"],
  expires_at: null,
  last_used_at: null,
  created_by: "u1",
  revoked_at: null,
  created_at: "2026-04-01T00:00:00Z",
  hlc: "",
};

const view: ApiKeysView = {
  keys: [portal],
  scopes: ["students", "attendance", "fees", "exams", "timetable", "library"],
  rate_limit_per_minute: 120,
  base_path: "/api/ro/v1",
};

function render() {
  return renderWithProviders(
    <FeaturesProvider
      initial={{ edition: "standard", features: ["dashboard", "settings", "readonly_api"] }}
    >
      <SessionProvider initial={{ user: admin, permissions: ["settings.manage"] }}>
        <ApiKeysCard />
      </SessionProvider>
    </FeaturesProvider>,
    { route: "/settings" },
  );
}

describe("ApiKeysCard", () => {
  beforeEach(() => {
    vi.mocked(api.apiKeys).mockResolvedValue(view);
  });

  it("lists keys, creates one and shows the secret once", async () => {
    vi.mocked(api.createApiKey).mockResolvedValue({
      ...portal,
      id: "k2",
      name: "BI",
      key: "mk_newsecretvalue",
    });
    render();
    expect(await screen.findByText("Portal")).toBeInTheDocument();
    expect(screen.getByText("Active")).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("add-key"));
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Name"), "BI");
    await userEvent.click(within(dialog).getByLabelText("Fees"));
    await userEvent.type(
      within(dialog).getByLabelText("Allowed addresses"),
      "10.0.0.0/8, 192.168.1.5",
    );
    await userEvent.click(within(dialog).getByTestId("save-key"));
    await waitFor(() =>
      expect(api.createApiKey).toHaveBeenCalledWith(
        expect.objectContaining({
          name: "BI",
          scopes: ["fees"],
          ip_allow: ["10.0.0.0/8", "192.168.1.5"],
          expires_at: null,
        }),
      ),
    );
    expect(await screen.findByTestId("new-key")).toHaveTextContent("mk_newsecretvalue");
  });

  it("revokes a key and shows the endpoint reference", async () => {
    vi.mocked(api.revokeApiKey).mockResolvedValue({
      ...portal,
      revoked_at: "2026-04-02T00:00:00Z",
    });
    vi.spyOn(window, "confirm").mockReturnValue(true);
    render();
    await screen.findByText("Portal");
    await userEvent.click(screen.getByRole("button", { name: "Revoke" }));
    await waitFor(() => expect(api.revokeApiKey).toHaveBeenCalledWith("k1"));
    await userEvent.click(screen.getByTestId("toggle-docs"));
    const docs = await screen.findByTestId("docs");
    expect(docs).toHaveTextContent("/attendance/daily");
    expect(docs).toHaveTextContent("curl");
  });
});
