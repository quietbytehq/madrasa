import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import type { RolloverPreview } from "@/api/types";
import { RolloverDialog } from "./RolloverDialog";

vi.mock("@/api/endpoints", () => ({
  api: {
    rolloverPreview: vi.fn(),
    rollover: vi.fn(),
    students: vi.fn(),
  },
}));

const preview: RolloverPreview = {
  old_year: {
    id: "y1",
    school_id: "s",
    name: "2026-27",
    start_date: "2026-04-01",
    end_date: "2027-03-31",
    start_month: 4,
    is_active: true,
  },
  new_year_name: "2027-28",
  new_start_year: 2027,
  moves: [
    {
      section_id: "s1",
      class_name: "1",
      section_name: "A",
      students: 30,
      target_class_id: "c2",
      target_class_name: "2",
    },
    {
      section_id: "s12",
      class_name: "12",
      section_name: "A",
      students: 25,
      target_class_id: null,
      target_class_name: null,
    },
  ],
  promoted: 30,
  graduating: 25,
  students_with_balance: 12,
  balance_carried: "48500.00",
  new_year_exists: false,
  months_into_year: 11,
};

describe("RolloverDialog", () => {
  it("previews the moves, resolves exceptions by student number and applies", async () => {
    vi.mocked(api.rolloverPreview).mockResolvedValue(preview);
    vi.mocked(api.students).mockImplementation(async (params) => {
      const q = params?.q;
      return {
        items:
          q === "KHI-MC-2026-00007"
            ? [{ id: "st7", student_number: "KHI-MC-2026-00007" } as never]
            : [],
        page: 1,
        page_size: 5,
        total: q === "KHI-MC-2026-00007" ? 1 : 0,
      };
    });
    vi.mocked(api.rollover).mockResolvedValue({
      new_year: { ...preview.old_year, id: "y2", name: "2027-28" },
      sections_created: 13,
      promoted: 29,
      retained: 1,
      graduated: 25,
      structure_lines_copied: 39,
      assessments_created: 4,
      balance_carried: "48500.00",
    });
    const onDone = vi.fn();
    renderWithProviders(<RolloverDialog onClose={() => {}} onDone={onDone} onError={() => {}} />);
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("2027-28")).toBeInTheDocument();
    expect(within(dialog).getByText(/30 students promoted, 25 graduate/)).toBeInTheDocument();
    expect(
      within(dialog).getByText("12 students carry 48,500.00 of unpaid fees into the new year"),
    ).toBeInTheDocument();
    const row12 = within(dialog).getByText("12-A").closest("tr")!;
    expect(row12).toHaveTextContent("Graduates");
    expect(within(dialog).queryByRole("alert")).not.toBeInTheDocument();

    await userEvent.type(
      within(dialog).getByLabelText("Repeat the class"),
      "KHI-MC-2026-00007\nNOPE",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: "Roll over now" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Unknown student numbers: NOPE",
    );
    expect(api.rollover).not.toHaveBeenCalled();

    await userEvent.clear(within(dialog).getByLabelText("Repeat the class"));
    await userEvent.type(within(dialog).getByLabelText("Repeat the class"), "KHI-MC-2026-00007");
    await userEvent.click(within(dialog).getByRole("button", { name: "Roll over now" }));
    await waitFor(() =>
      expect(api.rollover).toHaveBeenCalledWith({ retain: ["st7"], graduate: [] }),
    );
    expect(onDone).toHaveBeenCalledWith(
      "Rolled over to 2027-28: 29 promoted, 1 repeating, 25 graduated, 13 sections created.",
    );
  });

  it("warns when the year is young and blocks when the next year exists", async () => {
    vi.mocked(api.rolloverPreview).mockResolvedValue({
      ...preview,
      months_into_year: 2,
      new_year_exists: true,
    });
    renderWithProviders(<RolloverDialog onClose={() => {}} onDone={() => {}} onError={() => {}} />);
    const dialog = await screen.findByRole("dialog");
    const alerts = within(dialog).getAllByRole("alert");
    expect(alerts[0]).toHaveTextContent("started only 2 months ago");
    expect(alerts[1]).toHaveTextContent("already exists");
    expect(within(dialog).getByRole("button", { name: "Roll over now" })).toBeDisabled();
  });
});
