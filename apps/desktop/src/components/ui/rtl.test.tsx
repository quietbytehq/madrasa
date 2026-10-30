import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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

/**
 * RTL verification: every component renders under the Urdu locale with the document in
 * right-to-left mode, uses translated chrome strings, and only uses logical (start/end)
 * alignment so nothing is hard-wired to left/right.
 */
function Notify() {
  const { push } = useToast();
  return <Button onClick={() => push({ title: "ٹیسٹ", duration: 0 })}>toast</Button>;
}

function Everything() {
  return (
    <div>
      <Button icon={<span>i</span>}>بٹن</Button>
      <Input label="نام" hint="اشارہ" />
      <Select label="مہینہ" options={[{ value: "4", label: "اپریل" }]} />
      <Card title="کارڈ" actions={<Badge tone="info">نیا</Badge>}>
        <Spinner />
        <EmptyState title="خالی" />
      </Card>
      <Breadcrumbs items={[{ label: "گھر", to: "/" }, { label: "طلبہ" }]} />
      <Table
        caption="جدول"
        columns={[{ key: "name", header: "نام", sortable: true }]}
        rows={[{ id: "1", name: "علی" }]}
        rowKey={(r) => r.id}
        filterable
        pageSize={10}
      />
      <Dialog open onClose={() => {}} title="ڈائیلاگ">
        متن
      </Dialog>
      <Notify />
    </div>
  );
}

describe("RTL (Urdu) rendering of all components", () => {
  it("sets the document direction and renders translated chrome", async () => {
    renderWithProviders(<Everything />, { locale: "ur" });
    expect(document.documentElement.dir).toBe("rtl");
    expect(document.documentElement.lang).toBe("ur");

    expect(screen.getByRole("button", { name: /بٹن/ })).toBeInTheDocument();
    expect(screen.getByLabelText("نام")).toBeInTheDocument();
    expect(screen.getByLabelText("مہینہ")).toBeInTheDocument();
    expect(screen.getByRole("status", { name: "لوڈ ہو رہا ہے…" })).toBeInTheDocument();
    expect(screen.getByRole("searchbox", { name: "قطاریں فلٹر کریں" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "اگلا صفحہ" })).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "ڈائیلاگ" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "ڈائیلاگ بند کریں" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "toast" }));
    expect(screen.getByRole("button", { name: "اطلاع ہٹائیں" })).toBeInTheDocument();
  });

  it("uses only logical alignment classes (no left/right physical utilities)", () => {
    const { container } = renderWithProviders(<Everything />, { locale: "ur" });
    const html = container.innerHTML + document.body.innerHTML;
    // Physical Tailwind utilities that would break RTL mirroring.
    expect(html).not.toMatch(
      /\b(ml|mr|pl|pr|left|right|text-left|text-right|rounded-l|rounded-r)-/,
    );
  });
});
