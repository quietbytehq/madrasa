import { act, fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { renderWithProviders } from "@/test/render";
import {
  Badge,
  Breadcrumbs,
  Button,
  Card,
  Dialog,
  EmptyState,
  Input,
  Select,
  Spinner,
  Table,
  useToast,
} from "./index";
import type { Column } from "./Table";
import { compareValues } from "@/lib/compare";

describe("Button", () => {
  it("renders, defaults to type=button and disables while loading", () => {
    renderWithProviders(<Button loading>Save</Button>);
    const btn = screen.getByRole("button", { name: /save/i });
    expect(btn).toHaveAttribute("type", "button");
    expect(btn).toBeDisabled();
    expect(btn).toHaveAttribute("aria-busy", "true");
  });
  it("fires onClick", async () => {
    const onClick = vi.fn();
    renderWithProviders(<Button onClick={onClick}>Go</Button>);
    await userEvent.click(screen.getByRole("button", { name: "Go" }));
    expect(onClick).toHaveBeenCalledTimes(1);
  });
});

describe("Input", () => {
  it("links label, hint and error", () => {
    renderWithProviders(<Input label="Name" hint="Your full name" />);
    const input = screen.getByLabelText("Name");
    expect(input).toHaveAccessibleDescription("Your full name");
  });
  it("marks invalid when error given", () => {
    renderWithProviders(<Input label="Name" error="Required" />);
    expect(screen.getByLabelText("Name")).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("alert")).toHaveTextContent("Required");
  });
});

describe("Select", () => {
  it("renders options and placeholder", async () => {
    const onChange = vi.fn();
    renderWithProviders(
      <Select
        label="Month"
        placeholder="Pick"
        defaultValue=""
        onChange={onChange}
        options={[
          { value: "4", label: "April" },
          { value: "8", label: "August" },
        ]}
      />,
    );
    const select = screen.getByLabelText("Month");
    expect(within(select).getAllByRole("option")).toHaveLength(3);
    await userEvent.selectOptions(select, "8");
    expect(onChange).toHaveBeenCalled();
    expect((select as HTMLSelectElement).value).toBe("8");
  });
});

describe("Card / Badge / Spinner / EmptyState", () => {
  it("render their content", () => {
    renderWithProviders(
      <Card title="Students" actions={<Button size="sm">Add</Button>} footer="footer">
        <Badge tone="success">Active</Badge>
        <Spinner />
        <EmptyState title="Nothing here" description="Add something" />
      </Card>,
    );
    expect(screen.getByRole("heading", { name: "Students" })).toBeInTheDocument();
    expect(screen.getByText("Active")).toBeInTheDocument();
    expect(screen.getByRole("status", { name: "Loading…" })).toBeInTheDocument();
    expect(screen.getByText("Nothing here")).toBeInTheDocument();
    expect(screen.getByText("footer")).toBeInTheDocument();
  });
});

describe("Breadcrumbs", () => {
  it("links all but the current crumb", () => {
    renderWithProviders(
      <Breadcrumbs
        items={[
          { label: "Home", to: "/" },
          { label: "Students", to: "/students" },
          { label: "Ali" },
        ]}
      />,
    );
    expect(screen.getAllByRole("link")).toHaveLength(2);
    expect(screen.getByText("Ali")).toHaveAttribute("aria-current", "page");
  });
});

function DialogHarness() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button onClick={() => setOpen(true)}>Open</Button>
      <Dialog
        open={open}
        onClose={() => setOpen(false)}
        title="Confirm"
        description="Are you sure?"
      >
        <Input label="Reason" />
      </Dialog>
    </>
  );
}

describe("Dialog", () => {
  it("opens, focuses inside, closes with Escape and restores focus", async () => {
    renderWithProviders(<DialogHarness />);
    const opener = screen.getByRole("button", { name: "Open" });
    await userEvent.click(opener);
    const dialog = screen.getByRole("dialog", { name: "Confirm" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(dialog).toHaveAccessibleDescription("Are you sure?");
    expect(dialog.contains(document.activeElement)).toBe(true);
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(document.activeElement).toBe(opener);
  });
  it("closes via the close button", async () => {
    renderWithProviders(<DialogHarness />);
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    await userEvent.click(screen.getByRole("button", { name: "Close dialog" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});

function ToastHarness() {
  const { push } = useToast();
  return (
    <Button onClick={() => push({ title: "Saved", kind: "success", duration: 0 })}>Notify</Button>
  );
}

describe("Toast", () => {
  it("shows and dismisses a toast", async () => {
    renderWithProviders(<ToastHarness />);
    await userEvent.click(screen.getByRole("button", { name: "Notify" }));
    expect(screen.getByRole("status")).toHaveTextContent("Saved");
    await userEvent.click(screen.getByRole("button", { name: "Dismiss notification" }));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
  it("auto-dismisses after the duration", async () => {
    vi.useFakeTimers();
    try {
      function Auto() {
        const { push } = useToast();
        return <Button onClick={() => push({ title: "Bye", duration: 1000 })}>Go</Button>;
      }
      renderWithProviders(<Auto />);
      fireEvent.click(screen.getByRole("button", { name: "Go" }));
      expect(screen.getByRole("status")).toHaveTextContent("Bye");
      act(() => {
        vi.advanceTimersByTime(1100);
      });
      expect(screen.queryByRole("status")).not.toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });
});

interface Row {
  id: string;
  name: string;
  age: number;
}
const rows: Row[] = Array.from({ length: 23 }, (_, i) => ({
  id: String(i + 1),
  name: `Student ${String(i + 1).padStart(2, "0")}`,
  age: 20 - (i % 7),
}));
const columns: Column<Row>[] = [
  { key: "name", header: "Name", sortable: true },
  { key: "age", header: "Age", sortable: true, align: "end" },
];

describe("Table", () => {
  it("paginates", async () => {
    renderWithProviders(<Table columns={columns} rows={rows} rowKey={(r) => r.id} pageSize={10} />);
    expect(screen.getAllByRole("row")).toHaveLength(11); // header + 10
    expect(screen.getByText("Showing 1–10 of 23")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Next page" }));
    expect(screen.getByText("Showing 11–20 of 23")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Next page" }));
    expect(screen.getByText("Showing 21–23 of 23")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Next page" })).toBeDisabled();
  });

  it("sorts ascending, descending, then clears", async () => {
    renderWithProviders(<Table columns={columns} rows={rows} rowKey={(r) => r.id} pageSize={0} />);
    const header = screen.getByRole("button", { name: /^Age/ });
    const firstCell = () => screen.getAllByRole("row")[1]!.querySelectorAll("td")[1]!.textContent;
    await userEvent.click(header);
    expect(screen.getByRole("columnheader", { name: /Age/ })).toHaveAttribute(
      "aria-sort",
      "ascending",
    );
    expect(firstCell()).toBe("14");
    await userEvent.click(header);
    expect(firstCell()).toBe("20");
    await userEvent.click(header);
    expect(screen.getByRole("columnheader", { name: /Age/ })).toHaveAttribute("aria-sort", "none");
    expect(firstCell()).toBe("20"); // original order: Student 01 has age 20
    expect(screen.getAllByRole("row")[1]).toHaveTextContent("Student 01");
  });

  it("filters across columns and resets to page 1", async () => {
    renderWithProviders(
      <Table columns={columns} rows={rows} rowKey={(r) => r.id} filterable pageSize={10} />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Next page" }));
    await userEvent.type(screen.getByRole("searchbox"), "student 2");
    // Student 20, 21, 22, 23 (Student 02 does not contain "student 2")
    expect(screen.getAllByRole("row")).toHaveLength(5);
    expect(screen.getByText("Showing 1–4 of 4")).toBeInTheDocument();
  });

  it("shows an empty state", () => {
    renderWithProviders(<Table columns={columns} rows={[]} rowKey={(r) => r.id} />);
    expect(screen.getByText("Nothing to show")).toBeInTheDocument();
  });

  it("compareValues puts nulls last and sorts numerically", () => {
    expect(compareValues(null, 1)).toBeGreaterThan(0);
    expect(compareValues(2, 10)).toBeLessThan(0);
    expect(compareValues("item2", "item10")).toBeLessThan(0);
  });
});
