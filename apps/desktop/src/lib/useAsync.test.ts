import { afterEach, describe, expect, it } from "vitest";
import { ApiError } from "@/api/client";
import { errorMessage } from "./useAsync";

afterEach(() => {
  document.documentElement.lang = "en";
});

describe("errorMessage", () => {
  it("says the server's own words for a refusal that is about this request", () => {
    const err = new ApiError(400, "validation", "Amount must be greater than zero.");
    expect(errorMessage(err, "fallback")).toBe("Amount must be greater than zero.");
  });

  it("says a server fault, a missing permission and a lost sign-in in the person's language", () => {
    const fault = new ApiError(500, "internal", "Something went wrong on the school server.");
    const denied = new ApiError(403, "forbidden", "You do not have permission to do this.");
    const out = new ApiError(401, "unauthenticated", "Please log in to continue.");
    expect(errorMessage(fault, "fallback")).toMatch(
      /^Something went wrong on the school server\. Please try again\. If it happens again, note what you were doing/,
    );
    expect(errorMessage(denied, "fallback")).toBe(
      "You do not have permission to do this. Ask an administrator if you need it.",
    );
    expect(errorMessage(out, "fallback")).toBe(
      "You are signed out. Please sign in again to continue.",
    );
    document.documentElement.lang = "ur";
    expect(errorMessage(fault, "fallback")).toMatch(/^اسکول سرور پر کچھ غلط ہو گیا۔/);
    expect(errorMessage(denied, "fallback")).toMatch(/^آپ کو یہ کرنے کی اجازت نہیں۔/);
  });

  it("falls back when there is nothing to say", () => {
    expect(errorMessage(new Error("network"), "Could not reach the server")).toBe(
      "Could not reach the server",
    );
    expect(errorMessage(null, "fallback")).toBe("fallback");
  });
});
