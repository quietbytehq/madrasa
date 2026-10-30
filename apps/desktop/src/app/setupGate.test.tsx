import { act, screen } from "@testing-library/react";
import { Route, Routes } from "react-router-dom";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { SetupGate, useSetupGate } from "./SetupGate";

vi.mock("@/api/endpoints", () => ({
  api: { setupStatus: vi.fn() },
}));

function Wizard() {
  const { markCompleted } = useSetupGate();
  return (
    <div>
      wizard page
      <button onClick={markCompleted}>finish</button>
    </div>
  );
}

function renderGate(route: string) {
  return renderWithProviders(
    <SetupGate>
      <Routes>
        <Route path="/setup" element={<Wizard />} />
        <Route path="/login" element={<div>login page</div>} />
        <Route path="/" element={<div>home page</div>} />
      </Routes>
    </SetupGate>,
    { route },
  );
}

describe("SetupGate", () => {
  it("sends a fresh install to the wizard", async () => {
    vi.mocked(api.setupStatus).mockResolvedValue({ needs_setup: true });
    renderGate("/");
    expect(await screen.findByText("wizard page")).toBeInTheDocument();
  });

  it("sends a configured install away from the wizard", async () => {
    vi.mocked(api.setupStatus).mockResolvedValue({ needs_setup: false });
    renderGate("/setup");
    expect(await screen.findByText("login page")).toBeInTheDocument();
  });

  it("keeps the wizard visible after it completes so the recovery key can be read", async () => {
    vi.mocked(api.setupStatus).mockResolvedValue({ needs_setup: true });
    renderGate("/setup");
    const finish = await screen.findByText("finish");
    act(() => finish.click());
    expect(screen.getByText("wizard page")).toBeInTheDocument();
    expect(screen.queryByText("login page")).not.toBeInTheDocument();
  });

  it("falls through when the API cannot be reached", async () => {
    vi.mocked(api.setupStatus).mockImplementation(async () => {
      throw new Error("down");
    });
    renderGate("/");
    expect(await screen.findByText("home page")).toBeInTheDocument();
  });
});
