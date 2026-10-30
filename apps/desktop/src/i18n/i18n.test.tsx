import { act, render, screen } from "@testing-library/react";
import { I18nProvider, interpolate, translate, useI18n } from "./index";
import { en } from "./en";
import { ur } from "./ur";

describe("i18n dictionaries", () => {
  it("urdu has every english key and no extras", () => {
    const enKeys = Object.keys(en).sort();
    const urKeys = Object.keys(ur).sort();
    expect(urKeys).toEqual(enKeys);
  });

  it("urdu strings are non-empty and keep the same placeholders", () => {
    for (const [key, value] of Object.entries(en)) {
      const urValue = ur[key as keyof typeof ur];
      expect(urValue.trim().length, key).toBeGreaterThan(0);
      const enParams = (value.match(/\{\w+\}/g) ?? []).sort();
      const urParams = (urValue.match(/\{\w+\}/g) ?? []).sort();
      expect(urParams, key).toEqual(enParams);
    }
  });
});

describe("interpolate / translate", () => {
  it("fills placeholders", () => {
    expect(interpolate("Page {page} of {pages}", { page: 2, pages: 9 })).toBe("Page 2 of 9");
  });
  it("leaves unknown placeholders visible", () => {
    expect(interpolate("Hello {name}", {})).toBe("Hello {name}");
  });
  it("translates in both languages", () => {
    expect(translate("en", "nav.students")).toBe("Students");
    expect(translate("ur", "nav.students")).toBe("طلبہ");
  });
});

function Probe() {
  const { t, locale, dir, setLocale } = useI18n();
  return (
    <div>
      <span data-testid="locale">{locale}</span>
      <span data-testid="dir">{dir}</span>
      <span data-testid="text">{t("nav.dashboard")}</span>
      <button onClick={() => setLocale("ur")}>switch</button>
    </div>
  );
}

describe("I18nProvider", () => {
  it("switches language and document direction", () => {
    render(
      <I18nProvider initialLocale="en">
        <Probe />
      </I18nProvider>,
    );
    expect(screen.getByTestId("dir")).toHaveTextContent("ltr");
    expect(document.documentElement.dir).toBe("ltr");
    act(() => screen.getByText("switch").click());
    expect(screen.getByTestId("locale")).toHaveTextContent("ur");
    expect(screen.getByTestId("dir")).toHaveTextContent("rtl");
    expect(screen.getByTestId("text")).toHaveTextContent("ڈیش بورڈ");
    expect(document.documentElement.dir).toBe("rtl");
    expect(document.documentElement.lang).toBe("ur");
  });
});
