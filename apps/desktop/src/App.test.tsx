import { render, screen } from "@testing-library/react";
import { setBaseUrl } from "@/api/client";
import App from "./App";

describe("App", () => {
  it("boots and shows the loading state while the API is checked", () => {
    setBaseUrl("http://127.0.0.1:1");
    vi.stubGlobal(
      "fetch",
      vi.fn(() => new Promise(() => {})),
    );
    render(<App />);
    expect(screen.getByRole("status")).toBeInTheDocument();
    vi.unstubAllGlobals();
  });
});
