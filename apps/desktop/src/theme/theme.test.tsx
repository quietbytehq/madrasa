import { act, render, screen } from "@testing-library/react";
import { ThemeProvider, resolveTheme, useTheme } from "./index";

describe("resolveTheme", () => {
  it("follows the system when preference is system", () => {
    expect(resolveTheme("system", "dark")).toBe("dark");
    expect(resolveTheme("system", "light")).toBe("light");
  });
  it("overrides the system when a theme is chosen", () => {
    expect(resolveTheme("dark", "light")).toBe("dark");
    expect(resolveTheme("light", "dark")).toBe("light");
  });
});

function Probe() {
  const { theme, setPreference } = useTheme();
  return (
    <div>
      <span data-testid="theme">{theme}</span>
      <button onClick={() => setPreference("dark")}>dark</button>
    </div>
  );
}

describe("ThemeProvider", () => {
  it("applies the theme to the document", () => {
    render(
      <ThemeProvider initialPreference="light">
        <Probe />
      </ThemeProvider>,
    );
    expect(document.documentElement.dataset.theme).toBe("light");
    act(() => screen.getByText("dark").click());
    expect(screen.getByTestId("theme")).toHaveTextContent("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });
});
