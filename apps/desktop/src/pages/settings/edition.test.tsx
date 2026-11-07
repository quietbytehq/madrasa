import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { FeaturesProvider } from "@/app/features";
import { SessionProvider } from "@/app/session";
import type { EditionSettings, UserSummary } from "@/api/types";
import { EditionCard } from "./EditionCard";

vi.mock("@/api/endpoints", () => ({
  api: {
    editionSettings: vi.fn(),
    updateEdition: vi.fn(),
    features: vi.fn(),
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

const lite: EditionSettings = {
  edition: "lite",
  available: ["lite", "standard"],
  features: ["dashboard", "students"],
  standard_only: ["timetable", "leave", "library", "directory_login"],
};

describe("EditionCard", () => {
  it("shows the edition, what Standard adds, and switches", async () => {
    vi.mocked(api.editionSettings).mockResolvedValue(lite);
    vi.mocked(api.updateEdition).mockResolvedValue({ ...lite, edition: "standard" });
    vi.mocked(api.features).mockResolvedValue({
      edition: "standard",
      features: ["dashboard", "students", "timetable"],
      version: "0.2.1",
    });
    renderWithProviders(
      <FeaturesProvider initial={{ edition: "lite", features: ["dashboard", "settings"] }}>
        <SessionProvider initial={{ user: admin, permissions: ["settings.manage"] }}>
          <EditionCard />
        </SessionProvider>
      </FeaturesProvider>,
    );
    expect(await screen.findByTestId("edition-current")).toHaveTextContent("Lite");
    expect(screen.getByText("Timetable")).toBeInTheDocument();
    expect(screen.getByText("Directory login")).toBeInTheDocument();
    const button = screen.getByRole("button", { name: "Switch edition" });
    expect(button).toBeDisabled();
    await userEvent.selectOptions(screen.getByLabelText("Edition"), "standard");
    await userEvent.click(button);
    await waitFor(() => expect(api.updateEdition).toHaveBeenCalledWith("standard"));
    expect(await screen.findByTestId("edition-current")).toHaveTextContent("Standard");
    await waitFor(() => expect(api.features).toHaveBeenCalled());
  });

  it("cannot switch a Nexus instance", async () => {
    vi.mocked(api.editionSettings).mockResolvedValue({
      ...lite,
      edition: "nexus",
      available: ["nexus"],
    });
    renderWithProviders(
      <FeaturesProvider initial={{ edition: "nexus", features: ["dashboard"] }}>
        <SessionProvider initial={{ user: admin, permissions: ["settings.manage"] }}>
          <EditionCard />
        </SessionProvider>
      </FeaturesProvider>,
    );
    expect(await screen.findByText(/cannot be changed here/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Switch edition" })).not.toBeInTheDocument();
  });
});
