import { screen } from "@testing-library/react";
import { renderWithProviders } from "@/test/render";
import { BrandMark, BrandSplash } from "./Brand";

describe("brand", () => {
  it("shows the emblem as a decorative image", () => {
    renderWithProviders(<BrandMark className="h-7 w-7" />);
    const mark = screen.getByTestId("brand-mark");
    expect(mark.tagName).toBe("IMG");
    expect(mark).toHaveAttribute("alt", "");
    expect(mark).toHaveAttribute("aria-hidden", "true");
    // Small enough for Vite to inline.
    expect(mark.getAttribute("src")).toMatch(/madrasa-mark|^data:image\/svg\+xml/);
    expect(mark).toHaveClass("h-7", "w-7");
  });

  it("starts with the logo for light and dark themes, and a spinner", () => {
    renderWithProviders(<BrandSplash />);
    const logos = screen.getAllByAltText("Madrasa");
    expect(logos.map((l) => l.getAttribute("src"))).toEqual([
      expect.stringMatching(/madrasa-logo-vertical\.svg/),
      expect.stringMatching(/madrasa-logo-vertical-dark\.svg/),
    ]);
    expect(logos[0]).toHaveClass("dark:hidden");
    expect(logos[1]).toHaveClass("hidden", "dark:block");
    expect(screen.getByRole("status")).toBeInTheDocument();
  });
});
