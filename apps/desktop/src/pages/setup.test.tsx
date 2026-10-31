import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route, Routes } from "react-router-dom";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { ApiError } from "@/api/client";
import { FALLBACK_LISTS, Setup, applyCountry, toRequest, validateStep } from "./Setup";
import type { SetupForm } from "./Setup";

vi.mock("@/api/endpoints", () => ({
  api: { setup: vi.fn(), localeLists: vi.fn() },
}));

const filled: SetupForm = {
  schoolName: "Gulshan High",
  schoolCode: "ghs01",
  city: "Karachi",
  country: "PK",
  startMonth: "8",
  currency: "PKR",
  timeZone: "Asia/Karachi",
  defaultClasses: true,
  adminFullName: "Ayesha Khan",
  adminUsername: "ayesha",
  adminPassword: "admin-password-1",
  adminPasswordConfirm: "admin-password-1",
  backupPassword: "backup-password-1",
  backupPasswordConfirm: "backup-password-1",
};

describe("setup validation", () => {
  it("requires name and a well-formed code", () => {
    expect(validateStep(0, { ...filled, schoolName: " ", schoolCode: "a b" })).toEqual({
      schoolName: "setup.error.required",
      schoolCode: "setup.error.codeFormat",
    });
    expect(validateStep(0, filled)).toEqual({});
  });
  it("checks password length and confirmation", () => {
    expect(
      validateStep(2, { ...filled, adminPassword: "short", adminPasswordConfirm: "short" }),
    ).toEqual({
      adminPassword: "setup.error.passwordShort",
    });
    expect(validateStep(2, { ...filled, adminPasswordConfirm: "different-1" })).toEqual({
      adminPasswordConfirm: "setup.error.passwordMismatch",
    });
    expect(validateStep(3, { ...filled, backupPasswordConfirm: "x" })).toEqual({
      backupPasswordConfirm: "setup.error.passwordMismatch",
    });
  });
  it("maps the form to the API request", () => {
    expect(toRequest(filled)).toEqual({
      school_name: "Gulshan High",
      school_code: "GHS01",
      city: "Karachi",
      academic_year_start_month: 8,
      country: "PK",
      currency: "PKR",
      time_zone: "Asia/Karachi",
      first_year: true,
      default_classes: true,
      admin_full_name: "Ayesha Khan",
      admin_username: "ayesha",
      admin_password: "admin-password-1",
      backup_password: "backup-password-1",
    });
  });
  it("choosing a country sets its currency, time zone and year start", () => {
    const us = applyCountry(filled, "US", FALLBACK_LISTS);
    expect([us.country, us.currency, us.timeZone, us.startMonth]).toEqual([
      "US",
      "USD",
      "America/New_York",
      "8",
    ]);
    const back = applyCountry(us, "PK", FALLBACK_LISTS);
    expect([back.currency, back.timeZone, back.startMonth]).toEqual(["PKR", "Asia/Karachi", "4"]);
    expect(applyCountry(filled, "ZZ", FALLBACK_LISTS).country).toBe("ZZ");
  });
});

function renderWizard() {
  return renderWithProviders(
    <Routes>
      <Route path="/setup" element={<Setup />} />
      <Route path="/login" element={<div>login page</div>} />
    </Routes>,
    { route: "/setup" },
  );
}

async function fillStep1() {
  await userEvent.type(screen.getByLabelText("School name"), "Gulshan High");
  await userEvent.type(screen.getByLabelText("School code"), "ghs01");
  await userEvent.click(screen.getByRole("button", { name: "Continue" }));
}

describe("Setup wizard", () => {
  beforeEach(() => {
    vi.mocked(api.localeLists).mockResolvedValue(FALLBACK_LISTS);
  });
  // Note: vi.fn().mockReset() makes vitest report later rejections from the mock as
  // unhandled, so each test installs its own implementation instead.

  it("walks through the steps, shows the recovery key once and goes to login", async () => {
    vi.mocked(api.setup).mockResolvedValue({
      school_id: "s1",
      admin_user_id: "u1",
      recovery_key: "ABCD-EFGH-JKMN-PQRS-TVWX-YZ01-2345-6789",
    });
    renderWizard();

    // Step 1: validation then continue.
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getAllByRole("alert")).toHaveLength(2);
    await fillStep1();

    // Step 2: academic year.
    expect(screen.getByRole("heading", { name: "Academic year" })).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Academic year starts in"), "8");
    expect(screen.getByLabelText("Currency")).toHaveValue("PKR");
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));

    // Step 3: admin account.
    await userEvent.type(screen.getByLabelText("Full name"), "Ayesha Khan");
    await userEvent.type(screen.getByLabelText("Username"), "ayesha");
    await userEvent.type(screen.getByLabelText("Password"), "admin-password-1");
    await userEvent.type(screen.getByLabelText("Confirm password"), "admin-password-2");
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByRole("alert")).toHaveTextContent("The passwords do not match.");
    await userEvent.clear(screen.getByLabelText("Confirm password"));
    await userEvent.type(screen.getByLabelText("Confirm password"), "admin-password-1");
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));

    // Step 4: backup password + summary, then finish.
    expect(screen.getByText("Summary")).toBeInTheDocument();
    expect(screen.getByText("GHS01")).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Backup password"), "backup-password-1");
    await userEvent.type(screen.getByLabelText("Confirm backup password"), "backup-password-1");
    await userEvent.click(screen.getByRole("button", { name: "Finish" }));

    expect(api.setup).toHaveBeenCalledWith({
      school_name: "Gulshan High",
      school_code: "GHS01",
      city: "",
      academic_year_start_month: 8,
      country: "PK",
      currency: "PKR",
      time_zone: "Asia/Karachi",
      first_year: true,
      default_classes: true,
      admin_full_name: "Ayesha Khan",
      admin_username: "ayesha",
      admin_password: "admin-password-1",
      backup_password: "backup-password-1",
    });

    // Recovery key shown; must acknowledge before leaving.
    const key = await screen.findByLabelText("Recovery key");
    expect(key).toHaveTextContent("ABCD-EFGH-JKMN-PQRS-TVWX-YZ01-2345-6789");
    const go = screen.getByRole("button", { name: "Go to login" });
    expect(go).toBeDisabled();
    await userEvent.click(screen.getByRole("checkbox"));
    expect(go).toBeEnabled();
    await userEvent.click(go);
    expect(await screen.findByText("login page")).toBeInTheDocument();
  });

  it("shows the server's validation message", async () => {
    vi.mocked(api.setup).mockImplementation(async () => {
      throw new ApiError(400, "validation", "School code must be 2–12 letters, digits or hyphens.");
    });
    renderWizard();
    await fillStep1();
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    await userEvent.type(screen.getByLabelText("Full name"), "A");
    await userEvent.type(screen.getByLabelText("Username"), "a");
    await userEvent.type(screen.getByLabelText("Password"), "admin-password-1");
    await userEvent.type(screen.getByLabelText("Confirm password"), "admin-password-1");
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    await userEvent.type(screen.getByLabelText("Backup password"), "backup-password-1");
    await userEvent.type(screen.getByLabelText("Confirm backup password"), "backup-password-1");
    await userEvent.click(screen.getByRole("button", { name: "Finish" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "School code must be 2–12 letters, digits or hyphens.",
    );
  });
});
