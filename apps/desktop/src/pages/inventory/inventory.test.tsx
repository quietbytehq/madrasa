import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { FeaturesProvider } from "@/app/features";
import { SessionProvider } from "@/app/session";
import type { ItemDetail, ItemView, UserSummary } from "@/api/types";
import { InventoryPage } from "./InventoryPage";

vi.mock("@/api/endpoints", () => ({
  api: {
    inventoryCategories: vi.fn(),
    createInventoryCategory: vi.fn(),
    deleteInventoryCategory: vi.fn(),
    inventoryLocations: vi.fn(),
    createInventoryLocation: vi.fn(),
    deleteInventoryLocation: vi.fn(),
    inventoryItems: vi.fn(),
    inventoryItem: vi.fn(),
    createInventoryItem: vi.fn(),
    updateInventoryItem: vi.fn(),
    deleteInventoryItem: vi.fn(),
    inventoryStock: vi.fn(),
    inventoryMovements: vi.fn(),
    recordMovement: vi.fn(),
    deleteMovement: vi.fn(),
    inventoryTransfer: vi.fn(),
    inventoryRecipients: vi.fn(),
    inventoryAssets: vi.fn(),
    createAsset: vi.fn(),
    updateAsset: vi.fn(),
    deleteAsset: vi.fn(),
  },
}));

const admin: UserSummary = {
  id: "u1",
  school_id: "s",
  username: "bilal",
  full_name: "Bilal Store",
  email: null,
  is_active: true,
  roles: ["storekeeper"],
};

const chalk: ItemView = {
  id: "i1",
  school_id: "s",
  category_id: "c1",
  name: "Chalk (box)",
  code: "CH-1",
  unit: "box",
  reorder_level: "10.00",
  is_asset: false,
  active: true,
  notes: "",
  custom: {},
  hlc: "",
  category_name: "Stationery",
  on_hand: "8.00",
  below_reorder: true,
  assets: 0,
};

const detail: ItemDetail = {
  ...chalk,
  by_location: [
    {
      item_id: "i1",
      item_name: "Chalk (box)",
      code: "CH-1",
      unit: "box",
      category_name: "Stationery",
      location_id: "l1",
      location_name: "Main store",
      on_hand: "8.00",
    },
  ],
  recent: [],
  asset_list: [],
};

function renderPage(permissions: string[]) {
  return renderWithProviders(
    <FeaturesProvider
      initial={{ edition: "standard", features: ["dashboard", "students", "inventory"] }}
    >
      <SessionProvider initial={{ user: admin, permissions }}>
        <InventoryPage />
      </SessionProvider>
    </FeaturesProvider>,
    { route: "/inventory" },
  );
}

describe("InventoryPage", () => {
  beforeEach(() => {
    vi.mocked(api.inventoryCategories).mockResolvedValue([
      { id: "c1", school_id: "s", name: "Stationery", position: 0, hlc: "" },
    ]);
    vi.mocked(api.inventoryLocations).mockResolvedValue([
      {
        id: "l1",
        school_id: "s",
        name: "Main store",
        kind: "store",
        active: true,
        notes: "",
        hlc: "",
      },
      {
        id: "l2",
        school_id: "s",
        name: "Science lab",
        kind: "lab",
        active: true,
        notes: "",
        hlc: "",
      },
    ]);
    vi.mocked(api.inventoryItems).mockResolvedValue([chalk]);
    vi.mocked(api.inventoryItem).mockResolvedValue(detail);
  });

  it("lists items with the reorder flag and hides the store controls from readers", async () => {
    renderPage(["inventory.read"]);
    expect(screen.getAllByRole("tab")).toHaveLength(4);
    expect(await screen.findByText("Chalk (box)")).toBeInTheDocument();
    expect(screen.getByText("Reorder")).toBeInTheDocument();
    expect(screen.queryByTestId("record-movement")).not.toBeInTheDocument();
    expect(screen.queryByTestId("add-item")).not.toBeInTheDocument();
  });

  it("records an issue to a searched staff member and warns when stock is low", async () => {
    vi.mocked(api.inventoryRecipients).mockResolvedValue([
      { kind: "staff", id: "st1", name: "Zara Qureshi", detail: "Teacher" },
    ]);
    vi.mocked(api.recordMovement).mockResolvedValue({
      movement: {
        id: "m1",
        school_id: "s",
        item_id: "i1",
        location_id: "l1",
        kind: "issue",
        quantity: "2.00",
        unit_cost: null,
        total_cost: null,
        moved_on: "2026-04-10",
        recipient_kind: "staff",
        recipient_id: "st1",
        recipient_name: "Zara Qureshi",
        reference: "",
        transfer_id: null,
        notes: "",
        recorded_by: "u1",
        hlc: "",
        item_name: "Chalk (box)",
        unit: "box",
        location_name: "Main store",
        recorded_by_name: "Bilal Store",
      },
      on_hand: "6.00",
      below_reorder: true,
    });
    renderPage(["inventory.read", "inventory.manage"]);
    await screen.findByText("Chalk (box)");
    await userEvent.click(screen.getByTestId("record-movement"));
    const dialog = await screen.findByRole("dialog");
    await userEvent.selectOptions(within(dialog).getByLabelText("Kind"), "issue");
    await userEvent.selectOptions(within(dialog).getByLabelText("Item"), "i1");
    await userEvent.clear(within(dialog).getByLabelText("Quantity"));
    await userEvent.type(within(dialog).getByLabelText("Quantity"), "2");
    await userEvent.type(within(dialog).getByLabelText("Issued to"), "Za");
    await userEvent.click(await within(dialog).findByText("Zara Qureshi"));
    await userEvent.click(within(dialog).getByTestId("save-movement"));
    await waitFor(() =>
      expect(api.recordMovement).toHaveBeenCalledWith(
        expect.objectContaining({
          item_id: "i1",
          location_id: "l1",
          kind: "issue",
          quantity: "2",
          recipient_kind: "staff",
          recipient_id: "st1",
          recipient_name: "Zara Qureshi",
        }),
      ),
    );
    expect(await screen.findByText("Stock is at or below the reorder level.")).toBeInTheDocument();
  });

  it("adds a location from the setup tab", async () => {
    vi.mocked(api.createInventoryLocation).mockResolvedValue({
      id: "l3",
      school_id: "s",
      name: "Art room",
      kind: "room",
      active: true,
      notes: "",
      hlc: "",
    });
    renderPage(["inventory.read", "inventory.manage"]);
    await userEvent.click(screen.getByRole("tab", { name: "Setup" }));
    await userEvent.type(await screen.findByLabelText("Location"), "Art room");
    await userEvent.selectOptions(screen.getByLabelText("Kind"), "room");
    await userEvent.click(screen.getByTestId("add-location"));
    await waitFor(() =>
      expect(api.createInventoryLocation).toHaveBeenCalledWith(
        expect.objectContaining({ name: "Art room", kind: "room" }),
      ),
    );
  });
});
