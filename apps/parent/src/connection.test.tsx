import { act, render, screen, waitFor } from "@testing-library/react";
import { OFFLINE_EVENTS } from "@/api/client";
import { I18nProvider } from "@/i18n";
import { HubBadge } from "./connection";
import { REACHED } from "./api";

function renderBadge() {
  return render(
    <I18nProvider initialLocale="en">
      <HubBadge />
    </I18nProvider>,
  );
}

beforeEach(() => {
  localStorage.setItem("madrasa.parent.hub", "https://hub.example.org");
});

afterEach(() => {
  vi.unstubAllGlobals();
  localStorage.clear();
});

describe("the hub badge on every screen", () => {
  it("follows the hub's answers: connected, then reconnecting when a call fails", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() => Promise.resolve(new Response("{}", { status: 200 }))),
    );
    renderBadge();
    await waitFor(() => expect(screen.getByTestId("hub-link")).toHaveTextContent("Connected"));
    act(() => {
      window.dispatchEvent(new CustomEvent(OFFLINE_EVENTS.unreachable));
    });
    expect(screen.getByTestId("hub-link")).toHaveTextContent("Reconnecting");
    act(() => {
      window.dispatchEvent(new CustomEvent(REACHED));
    });
    expect(screen.getByTestId("hub-link")).toHaveTextContent("Connected");
  });

  it("says offline at once when the phone has no network", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() => Promise.reject(new TypeError("Failed to fetch"))),
    );
    const online = vi.spyOn(navigator, "onLine", "get").mockReturnValue(false);
    renderBadge();
    await waitFor(() => expect(screen.getByTestId("hub-link")).toHaveTextContent("Offline"));
    online.mockRestore();
  });
});
