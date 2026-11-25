import { screen, waitFor } from "@testing-library/react";
import { Route, Routes } from "react-router-dom";
import { renderWithProviders } from "@/test/render";
import { request, setBaseUrl, setToken, getToken, ApiError } from "@/api/client";
import { SessionProvider } from "@/app/session";
import { RequireAuth } from "@/app/router";
import type { UserSummary } from "@/api/types";

const user: UserSummary = {
  id: "u1",
  school_id: "s1",
  username: "bilal",
  full_name: "Bilal Ahmed",
  email: null,
  is_active: true,
  roles: ["teacher"],
};

function json(status: number, body: unknown) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function renderApp() {
  return renderWithProviders(
    <SessionProvider initial={{ user, permissions: ["students.read"] }}>
      <Routes>
        <Route path="/login" element={<p>login page</p>} />
        <Route
          path="/"
          element={
            <RequireAuth>
              <p>the school&apos;s records</p>
            </RequireAuth>
          }
        />
      </Routes>
    </SessionProvider>,
  );
}

// Found in the lab (security review after v0.5.1): the password was changed from another
// session, the server ended this one, and the app stayed on its screen showing errors.
describe("a sign-in the server has ended", () => {
  beforeEach(() => {
    setBaseUrl("http://api");
    setToken("a-token-the-server-no-longer-knows");
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    setToken(null);
  });

  it("sends the app back to the login screen and forgets the token", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        json(401, { error: { code: "unauthenticated", message: "Please log in to continue." } }),
      ),
    );
    renderApp();
    expect(screen.getByText("the school's records")).toBeInTheDocument();
    await expect(request("/students")).rejects.toBeInstanceOf(ApiError);
    await waitFor(() => expect(screen.getByText("login page")).toBeInTheDocument());
    expect(screen.queryByText("the school's records")).toBeNull();
    expect(getToken()).toBeNull();
  });

  it("leaves a refused permission, and a wrong password at the login, alone", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) =>
        String(input).endsWith("/auth/login")
          ? json(401, { error: { code: "invalid_credentials", message: "wrong" } })
          : json(403, { error: { code: "forbidden", message: "no" } }),
      ),
    );
    renderApp();
    await expect(request("/fees/vouchers")).rejects.toBeInstanceOf(ApiError);
    await expect(
      request("/auth/login", { method: "POST", body: {}, auth: false }),
    ).rejects.toBeInstanceOf(ApiError);
    await new Promise((r) => setTimeout(r, 30));
    expect(screen.getByText("the school's records")).toBeInTheDocument();
    expect(getToken()).toBe("a-token-the-server-no-longer-knows");
  });
});
