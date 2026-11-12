import {
  HubError,
  OfflineError,
  hubApi,
  normalizeHubUrl,
  setHubUrl,
  setSignedOutHandler,
  setToken,
} from "./api";

const HUB = "https://hub.example.org";

function answer(status: number, body: unknown) {
  return Promise.resolve(
    new Response(typeof body === "string" ? body : JSON.stringify(body), { status }),
  );
}

describe("hub address", () => {
  it("accepts what is printed on the slip, however it is typed", () => {
    expect(normalizeHubUrl("hub.example.org")).toBe(HUB);
    expect(normalizeHubUrl("  https://hub.example.org/  ")).toBe(HUB);
    expect(normalizeHubUrl("HTTPS://Hub.Example.org/api/nexus/v1")).toBe(HUB);
    expect(normalizeHubUrl("hub.example.org:8443")).toBe("https://hub.example.org:8443");
  });

  it("refuses plain http except to this device, and anything that is not an address", () => {
    expect(normalizeHubUrl("http://hub.example.org")).toBeNull();
    expect(normalizeHubUrl("http://192.168.1.20:4780")).toBeNull();
    expect(normalizeHubUrl("http://localhost:4780")).toBe("http://localhost:4780");
    expect(normalizeHubUrl("http://127.0.0.1:4780/")).toBe("http://127.0.0.1:4780");
    expect(normalizeHubUrl("http://10.0.2.2:4780")).toBe("http://10.0.2.2:4780");
    for (const bad of [
      "",
      "   ",
      "my school",
      "ftp://hub.example.org",
      "hub",
      "https://u:p@hub.example.org",
    ]) {
      expect(normalizeHubUrl(bad), bad).toBeNull();
    }
  });
});

describe("calls to the hub", () => {
  const fetchMock = vi.fn();
  beforeEach(() => {
    localStorage.clear();
    fetchMock.mockReset();
    vi.stubGlobal("fetch", fetchMock);
    setHubUrl(HUB);
    setToken("the-token");
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    setSignedOutHandler(null);
  });

  it("sends the token in a header, never cookies, to the stored hub", async () => {
    fetchMock.mockReturnValue(answer(200, { children: [] }));
    await expect(hubApi.children()).resolves.toEqual({ children: [] });
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(url).toBe(`${HUB}/api/nexus/v1/parent/children`);
    expect(init.headers.Authorization).toBe("Bearer the-token");
    expect(init.credentials).toBe("omit");
  });

  it("asks about a code without a token, at the address just typed", async () => {
    fetchMock.mockReturnValue(
      answer(200, { guardian_name: "Imran Khan", school_name: "Gulshan High", children: ["Ali"] }),
    );
    await hubApi.checkCode("https://other.example.org", "ABCD-EFGH-JKMN");
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(url).toBe("https://other.example.org/api/nexus/v1/parent/activate/check");
    expect(init.headers.Authorization).toBeUndefined();
    expect(JSON.parse(init.body)).toEqual({ code: "ABCD-EFGH-JKMN" });
  });

  it("names a child by school and student in the path", async () => {
    fetchMock.mockReturnValue(answer(200, { month: "2026-05", days: [] }));
    await hubApi.attendance({ school: "sch-1", student: "stu-1" }, "2026-05");
    expect(fetchMock.mock.calls[0]![0]).toBe(
      `${HUB}/api/nexus/v1/parent/children/sch-1/stu-1/attendance?month=2026-05`,
    );
  });

  it("turns a refusal into a HubError with the hub's code", async () => {
    fetchMock.mockReturnValue(
      answer(404, { error: { code: "not_found", message: "The child was not found." } }),
    );
    const err = await hubApi.fees({ school: "s", student: "x" }).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(HubError);
    expect(err).toMatchObject({ status: 404, code: "not_found" });
  });

  it("signs the app out when the hub no longer knows the session", async () => {
    const signedOut = vi.fn();
    setSignedOutHandler(signedOut);
    fetchMock.mockReturnValue(
      answer(401, { error: { code: "unauthenticated", message: "Sign in." } }),
    );
    await expect(hubApi.children()).rejects.toBeInstanceOf(HubError);
    expect(signedOut).toHaveBeenCalledTimes(1);
    // A wrong password is not a lost session.
    fetchMock.mockReturnValue(
      answer(401, { error: { code: "invalid_credentials", message: "Wrong." } }),
    );
    await expect(hubApi.login(HUB, "0300", "x")).rejects.toBeInstanceOf(HubError);
    expect(signedOut).toHaveBeenCalledTimes(1);
  });

  it("says offline when nothing answers, or something that is not a hub does", async () => {
    fetchMock.mockRejectedValue(new TypeError("Failed to fetch"));
    await expect(hubApi.children()).rejects.toBeInstanceOf(OfflineError);
    fetchMock.mockReturnValue(answer(200, "<html>Welcome to nginx</html>"));
    await expect(hubApi.children()).rejects.toBeInstanceOf(OfflineError);
    setHubUrl(null);
    await expect(hubApi.children()).rejects.toBeInstanceOf(OfflineError);
  });
});
