import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { FeaturesProvider } from "@/app/features";
import { SessionProvider } from "@/app/session";
import type { TimetableSetup, TimetableShift, TimetableView, UserSummary } from "@/api/types";
import { TimetablePage } from "./TimetablePage";
import { MyTimetableCard } from "./MyTimetableCard";

vi.mock("@/api/endpoints", () => ({
  api: {
    timetableSetup: vi.fn(),
    currentTimetable: vi.fn(),
    timetable: vi.fn(),
    myTimetable: vi.fn(),
    createDefaultShift: vi.fn(),
    slotOptions: vi.fn(),
    swapOptions: vi.fn(),
    moveCard: vi.fn(),
    printTimetable: vi.fn(),
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

const shift: TimetableShift = {
  id: "sh",
  school_id: "s",
  academic_year_id: "y",
  name: "Morning",
  enabled: true,
  days: [
    { working: true, periods: 2 },
    { working: true, periods: 2 },
    { working: false, periods: 0 },
    { working: false, periods: 0 },
    { working: false, periods: 0 },
    { working: false, periods: 0 },
    { working: false, periods: 0 },
  ],
  period_times: [
    { start: "08:00", end: "08:40" },
    { start: "08:40", end: "09:20" },
  ],
  breaks: [],
  position: 0,
  hlc: "",
};

const year = {
  id: "y",
  school_id: "s",
  name: "2026-27",
  start_date: "2026-04-01",
  end_date: "2027-03-31",
  start_month: 4,
  is_active: true,
} as TimetableSetup["academic_year"];

const lesson = {
  id: "l1",
  school_id: "s",
  academic_year_id: "y",
  subject_id: "m",
  teacher_ids: ["t1"],
  section_ids: ["c1"],
  room_ids: [],
  periods_per_card: 1,
  cards_per_week: 2,
  hlc: "",
  subject: "Mathematics",
  subject_short: "MATH",
  teachers: ["Sana Malik"],
  sections: ["9 A"],
  rooms: [],
};

const setup: TimetableSetup = {
  academic_year: year,
  shifts: [shift],
  rooms: [],
  lessons: [lesson],
  rules: [],
  sections: [{ id: "c1", name: "9 A", class_id: "k" }],
  teachers: [{ id: "t1", name: "Sana Malik" }],
  subjects: [{ id: "m", name: "Mathematics", code: "MATH" }],
  timetables: [
    {
      id: "tt",
      school_id: "s",
      academic_year_id: "y",
      name: "Draft 1",
      status: "published",
      report: null,
      generated_at: null,
      created_at: "2026-04-01T00:00:00Z",
      hlc: "",
    },
  ],
  published_id: "tt",
  check: {
    errors: 0,
    warnings: 1,
    cards: 2,
    periods: 4,
    findings: [
      {
        level: "warning",
        kind: "room",
        title: "No rooms",
        message: "No rooms are set up.",
        fix: "Add rooms.",
      },
    ],
  },
  load: {
    cards: 2,
    classes: [{ id: "c1", name: "9 A", used: 2, available: 4 }],
    teachers: [{ id: "t1", name: "Sana Malik", used: 2, available: 4 }],
  },
  school_rules: {
    default_max_per_day: null,
    default_max_per_week: null,
    max_same_subject_per_day: 2,
    allow_relaxation: false,
    improve_seconds: 0,
  },
};

const view: TimetableView = {
  timetable: setup.timetables[0]!,
  academic_year: year,
  shifts: [shift],
  lessons: [lesson],
  cards: [
    {
      id: "a",
      timetable_id: "tt",
      lesson_id: "l1",
      number: 1,
      shift_id: "sh",
      day: 0,
      period: 1,
      length: 1,
      room_id: null,
      locked: false,
    },
    {
      id: "b",
      timetable_id: "tt",
      lesson_id: "l1",
      number: 2,
      shift_id: "sh",
      day: 1,
      period: 2,
      length: 1,
      room_id: null,
      locked: true,
    },
  ],
  rooms: [],
  sections: setup.sections,
  teachers: setup.teachers,
  problems: [],
  job: null,
  covers: [],
  today: 1,
};

function renderPage(permissions: string[]) {
  return renderWithProviders(
    <FeaturesProvider initial={{ edition: "standard", features: ["dashboard", "timetable"] }}>
      <SessionProvider initial={{ user: admin, permissions }}>
        <TimetablePage />
      </SessionProvider>
    </FeaturesProvider>,
    { route: "/timetable" },
  );
}

describe("TimetablePage", () => {
  beforeEach(() => {
    vi.mocked(api.timetableSetup).mockResolvedValue(setup);
    vi.mocked(api.currentTimetable).mockResolvedValue(view);
    vi.mocked(api.timetable).mockResolvedValue(view);
  });

  it("shows the published timetable grid by section and lets the office open a session", async () => {
    vi.mocked(api.slotOptions).mockResolvedValue([
      { shift: "sh", day: 1, period: 1, status: "ok", reasons: [], rooms: [] },
      {
        shift: "sh",
        day: 0,
        period: 2,
        status: "blocked",
        reasons: ["Sana Malik is busy."],
        rooms: [],
      },
    ]);
    vi.mocked(api.swapOptions).mockResolvedValue([]);
    vi.mocked(api.moveCard).mockResolvedValue({
      ...view,
      cards: [{ ...view.cards[0]!, day: 1, period: 1 }, view.cards[1]!],
    });
    renderPage(["timetable.read", "timetable.manage"]);
    const grid = await screen.findByTestId("timetable-grid");
    expect(within(grid).getAllByText("Mathematics")).toHaveLength(2);
    expect(within(grid).getByText("Monday")).toBeInTheDocument();
    expect(within(grid).getByText("Tuesday")).toBeInTheDocument();
    expect(within(grid).queryByText("Wednesday")).not.toBeInTheDocument();
    // Office roles see all five tabs; the session dialog offers the free slot only.
    expect(screen.getAllByRole("tab")).toHaveLength(5);
    await userEvent.click(screen.getByTestId("cell-l1-1"));
    const options = await screen.findByTestId("slot-options");
    expect(within(options).getByText("Tuesday 1")).toBeInTheDocument();
    expect(within(options).queryByText("Monday 2")).not.toBeInTheDocument();
    await userEvent.click(within(options).getByRole("button", { name: "Move here" }));
    await waitFor(() =>
      expect(api.moveCard).toHaveBeenCalledWith("tt", {
        lesson_id: "l1",
        number: 1,
        day: 1,
        period: 1,
        room_id: null,
      }),
    );
  });

  it("hides the setup tabs from a teacher and shows the pre-check to the office", async () => {
    renderPage(["timetable.read"]);
    await screen.findByTestId("timetable-grid");
    expect(screen.getAllByRole("tab")).toHaveLength(2);
    expect(screen.queryByRole("tab", { name: "Lessons" })).not.toBeInTheDocument();
  });

  it("lists lessons with their load and the check findings", async () => {
    renderPage(["timetable.read", "timetable.manage"]);
    await screen.findByTestId("timetable-grid");
    await userEvent.click(screen.getByRole("tab", { name: "Lessons" }));
    expect(await screen.findByText("No rooms are set up.")).toBeInTheDocument();
    expect(screen.getAllByText("1 warnings").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Sana Malik").length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: "Seed from allocations" })).toBeInTheDocument();
  });

  it("offers the default morning shift when no shift exists", async () => {
    vi.mocked(api.timetableSetup).mockResolvedValue({ ...setup, shifts: [] });
    vi.mocked(api.createDefaultShift).mockResolvedValue(shift);
    renderPage(["timetable.read", "timetable.manage"]);
    await screen.findByRole("tab", { name: "Setup" });
    await userEvent.click(screen.getByRole("tab", { name: "Setup" }));
    await userEvent.click(await screen.findByRole("button", { name: "Add morning shift" }));
    await waitFor(() => expect(api.createDefaultShift).toHaveBeenCalledWith("y"));
  });
});

describe("MyTimetableCard", () => {
  it("shows the teacher's own week with today marked, or nothing without a timetable", async () => {
    vi.mocked(api.myTimetable).mockResolvedValue({
      ...view,
      covers: [
        {
          date: "2026-04-07",
          shift_id: "sh",
          period: 2,
          length: 1,
          lesson_id: "l2",
          subject: "Urdu",
          sections: ["9 B"],
          absent_teacher: "Bilal Ahmed",
        },
      ],
    });
    renderWithProviders(<MyTimetableCard />);
    const card = await screen.findByTestId("my-timetable");
    expect(within(card).getAllByText("Mathematics")).toHaveLength(2);
    expect(within(card).getByText("Cover duties")).toBeInTheDocument();
    expect(within(card).getByText(/Urdu/)).toBeInTheDocument();
  });

  it("renders nothing when the login has no staff record", async () => {
    vi.mocked(api.myTimetable).mockRejectedValue(new Error("not found"));
    renderWithProviders(<MyTimetableCard />);
    await waitFor(() => expect(api.myTimetable).toHaveBeenCalled());
    expect(screen.queryByTestId("my-timetable")).not.toBeInTheDocument();
  });
});
