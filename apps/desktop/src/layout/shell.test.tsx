import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { FeaturesProvider } from "@/app/features";
import { SessionProvider } from "@/app/session";
import { AppRoutes } from "@/app/router";
import type { UserSummary } from "@/api/types";

const user: UserSummary = {
  id: "u1",
  school_id: "s1",
  username: "admin",
  full_name: "Ayesha Khan",
  email: null,
  is_active: true,
  roles: ["admin"],
};

function renderShell(edition: "lite" | "nexus" = "lite", route = "/") {
  const features =
    edition === "lite"
      ? ([
          "dashboard",
          "students",
          "attendance",
          "examinations",
          "fees",
          "teachers",
          "reports",
          "backup",
          "settings",
        ] as const)
      : ([
          "dashboard",
          "students",
          "attendance",
          "examinations",
          "fees",
          "teachers",
          "reports",
          "backup",
          "settings",
          "campuses",
          "parents_portal",
          "nexus_hub",
        ] as const);
  return renderWithProviders(
    <FeaturesProvider initial={{ edition, features: [...features] }}>
      <SessionProvider initial={{ user, permissions: ["school.read"] }}>
        <AppRoutes login={<div>login page</div>} setup={<div>setup page</div>} />
      </SessionProvider>
    </FeaturesProvider>,
    { route },
  );
}

describe("App shell", () => {
  it("shows the Lite modules and hides edition-gated ones", () => {
    renderShell("lite");
    const nav = screen.getByRole("navigation", { name: "Modules" });
    const labels = within(nav)
      .getAllByRole("link")
      .map((l) => l.textContent);
    expect(labels).toEqual([
      "Dashboard",
      "Students",
      "Import",
      "Attendance",
      "Examinations",
      "Fees & Finance",
      "Teachers",
      "Reports",
      "Backup",
      "Settings",
    ]);
  });

  it("shows Campuses, Parents and Network for Nexus", () => {
    renderShell("nexus");
    const nav = screen.getByRole("navigation", { name: "Modules" });
    expect(within(nav).getByRole("link", { name: "Campuses" })).toBeInTheDocument();
    expect(within(nav).getByRole("link", { name: "Parents" })).toBeInTheDocument();
    expect(within(nav).getByRole("link", { name: "Network" })).toBeInTheDocument();
  });

  it("renders the empty dashboard with placeholder cards", () => {
    renderShell();
    expect(screen.getByRole("heading", { level: 1, name: "Dashboard" })).toBeInTheDocument();
    expect(screen.getByText("Welcome, Ayesha Khan")).toBeInTheDocument();
    expect(screen.getAllByText("Data will appear once the module is available.")).toHaveLength(4);
    expect(screen.getByText("Edition: Lite")).toBeInTheDocument();
  });

  it("navigates to a Coming soon module via the sidebar", async () => {
    renderShell();
    await userEvent.click(screen.getByRole("link", { name: "Reports" }));
    expect(screen.getByRole("heading", { level: 1, name: "Reports" })).toBeInTheDocument();
    expect(screen.getAllByText("Coming soon").length).toBeGreaterThan(0);
  });

  it("opens search with Ctrl+K", async () => {
    renderShell();
    await userEvent.keyboard("{Control>}k{/Control}");
    expect(screen.getByRole("dialog", { name: "Search" })).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("user menu switches language and logs out", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(null, { status: 204 })),
    );
    try {
      renderShell();
      await userEvent.click(screen.getByRole("button", { name: "User menu" }));
      await userEvent.click(screen.getByRole("menuitem", { name: "اردو" }));
      expect(document.documentElement.dir).toBe("rtl");
      expect(screen.getByRole("navigation", { name: "ماڈیولز" })).toBeInTheDocument();
      await userEvent.click(screen.getByRole("menuitem", { name: "لاگ آؤٹ" }));
      expect(await screen.findByText("login page")).toBeInTheDocument();
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("collapses the sidebar and keeps items reachable by keyboard", async () => {
    renderShell();
    await userEvent.click(screen.getByRole("button", { name: "Collapse sidebar" }));
    expect(screen.getByRole("button", { name: "Expand sidebar" })).toBeInTheDocument();
    // Collapsed items keep an accessible name even though the text is hidden.
    expect(screen.getByRole("link", { name: "Students" })).toBeInTheDocument();
    // Keyboard order starts with the skip link, then the sidebar links.
    (document.activeElement as HTMLElement | null)?.blur();
    await userEvent.tab();
    expect(document.activeElement).toHaveTextContent("Skip to content");
    await userEvent.tab();
    expect(document.activeElement).toHaveAttribute("href", "/");
    await userEvent.tab();
    expect(document.activeElement).toHaveAttribute("href", "/students");
  });

  it("redirects anonymous users to login", () => {
    renderWithProviders(
      <FeaturesProvider initial={{ edition: "lite", features: ["dashboard"] }}>
        <SessionProvider initial="anonymous">
          <AppRoutes login={<div>login page</div>} setup={<div />} />
        </SessionProvider>
      </FeaturesProvider>,
      { route: "/" },
    );
    expect(screen.getByText("login page")).toBeInTheDocument();
  });
});
