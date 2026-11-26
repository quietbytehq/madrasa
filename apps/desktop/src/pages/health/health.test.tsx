import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import type { HealthCheck } from "@/api/types";
import { DataHealthPage } from "./DataHealthPage";

vi.mock("@/api/endpoints", () => ({
  api: { dataHealth: vi.fn() },
}));

const checks: HealthCheck[] = [
  {
    code: "students_without_section",
    severity: "problem",
    area: "students",
    count: 27,
    sample: [
      { label: "Omar Khan", detail: "KHI-2026-00012", link: "/students/s1" },
      { label: "Sara Khan", detail: "KHI-2026-00013", link: "/students/s2" },
    ],
  },
  { code: "duplicate_student_names", severity: "warning", area: "students", count: 0, sample: [] },
  {
    code: "guardian_phone_shared",
    severity: "warning",
    area: "guardians",
    count: 1,
    sample: [{ label: "03001234567", detail: "Imran Raza, Imran Reza", link: null }],
  },
  {
    code: "classes_without_fee_structure",
    severity: "problem",
    area: "fees",
    count: 0,
    sample: [],
  },
];

describe("DataHealthPage", () => {
  it("says what needs attention and why, links to the records, and says what is in order", async () => {
    vi.mocked(api.dataHealth).mockResolvedValue(checks);
    renderWithProviders(<DataHealthPage />);
    expect(await screen.findByTestId("health-summary")).toHaveTextContent(
      "Needs attention — problems: 1, warnings: 1.",
    );
    const noSection = within(screen.getByTestId("health-students_without_section"));
    expect(
      noSection.getByRole("heading", {
        name: "Active students without a class and section this year",
      }),
    ).toBeInTheDocument();
    expect(noSection.getByText(/get no fee voucher/)).toBeInTheDocument();
    expect(screen.getByTestId("health-count-students_without_section")).toHaveTextContent("27");
    expect(noSection.getByRole("link", { name: "Omar Khan" })).toHaveAttribute(
      "href",
      "/students/s1",
    );
    expect(noSection.getByText("and 25 more")).toBeInTheDocument();

    // A finding without a place to go is shown as text.
    const shared = within(screen.getByTestId("health-guardian_phone_shared"));
    expect(shared.getByText("03001234567")).toBeInTheDocument();
    expect(shared.queryByRole("link")).not.toBeInTheDocument();

    // What is in order is listed too, without the explanation.
    const names = within(screen.getByTestId("health-duplicate_student_names"));
    expect(names.getByText("Nothing found")).toBeInTheDocument();
    expect(names.queryByText(/same child entered twice/)).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Fees" })).toBeInTheDocument();

    // Checking again asks the server again.
    vi.mocked(api.dataHealth).mockResolvedValue(
      checks.map((c) => ({ ...c, count: 0, sample: [] })),
    );
    await userEvent.click(screen.getByRole("button", { name: "Check again" }));
    expect(await screen.findByText("Nothing needs attention.")).toBeInTheDocument();
    expect(api.dataHealth).toHaveBeenCalledTimes(2);
  });

  it("says so plainly when the checks cannot be run", async () => {
    vi.mocked(api.dataHealth).mockRejectedValue(new Error("network"));
    renderWithProviders(<DataHealthPage />);
    expect(await screen.findByText("Could not load this page. Try refreshing.")).toBeVisible();
  });
});
