import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { getMode, networkInfo, setMode } from "@/app/mode";
import { NetworkCard } from "./NetworkCard";

vi.mock("@/app/mode", async () => {
  const actual = await vi.importActual<typeof import("@/app/mode")>("@/app/mode");
  return {
    ...actual,
    getMode: vi.fn(),
    networkInfo: vi.fn(),
    setMode: vi.fn(async (config) => config),
    discoverServers: vi.fn(async () => []),
    probeServer: vi.fn(),
  };
});

const FP = "AB:CD".repeat(16).slice(0, 95);

beforeEach(() => {
  vi.mocked(setMode).mockClear();
});

describe("NetworkCard", () => {
  it("shows an upgraded standalone install and switches it to the school server", async () => {
    vi.mocked(getMode).mockResolvedValue({
      mode: "standalone",
      server_url: null,
      fingerprint: null,
      listen_port: 8471,
    });
    vi.mocked(networkInfo).mockResolvedValue({
      mode: "standalone",
      addresses: [],
      port: 0,
      fingerprint: null,
      server_url: null,
    });
    renderWithProviders(<NetworkCard />);
    expect(await screen.findByText("Just this PC")).toBeInTheDocument();
    expect(screen.getByText(/make this PC the school server/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Change" }));
    expect(screen.getByText(/Madrasa restarts after the change/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Just this PC · current/ })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /This PC is the school server/ }));
    expect(screen.getByText(/netsh advfirewall/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Start as the school server" }));
    await waitFor(() =>
      expect(setMode).toHaveBeenCalledWith({
        mode: "server",
        server_url: null,
        fingerprint: null,
        listen_port: 8471,
      }),
    );
    expect(await screen.findByText(/restarting in the new mode/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Change" })).not.toBeInTheDocument();
  });

  it("shows a server's addresses, port and certificate fingerprint", async () => {
    vi.mocked(getMode).mockResolvedValue({
      mode: "server",
      server_url: null,
      fingerprint: null,
      listen_port: 8471,
    });
    vi.mocked(networkInfo).mockResolvedValue({
      mode: "server",
      addresses: ["192.168.1.10"],
      port: 8471,
      fingerprint: FP,
      server_url: null,
    });
    renderWithProviders(<NetworkCard />);
    expect(await screen.findByText("This PC is the school server")).toBeInTheDocument();
    expect(screen.getByText("192.168.1.10")).toBeInTheDocument();
    expect(screen.getByText("8471")).toBeInTheDocument();
    expect(screen.getByText(FP)).toBeInTheDocument();
    expect(screen.getByText("Windows Firewall")).toBeInTheDocument();
  });

  it("shows which server a client uses and is absent in the browser build", async () => {
    vi.mocked(getMode).mockResolvedValue({
      mode: "client",
      server_url: "https://192.168.1.10:8471",
      fingerprint: FP,
      listen_port: 8471,
    });
    vi.mocked(networkInfo).mockResolvedValue({
      mode: "client",
      addresses: [],
      port: 0,
      fingerprint: FP,
      server_url: "https://192.168.1.10:8471",
    });
    const { unmount } = renderWithProviders(<NetworkCard />);
    expect(await screen.findByText("Connect to a school server")).toBeInTheDocument();
    expect(screen.getByText("https://192.168.1.10:8471")).toBeInTheDocument();
    expect(screen.getByText(FP)).toBeInTheDocument();
    unmount();

    vi.mocked(getMode).mockResolvedValue(null);
    vi.mocked(networkInfo).mockResolvedValue(null);
    renderWithProviders(<NetworkCard />);
    await waitFor(() => expect(getMode).toHaveBeenCalled());
    expect(screen.queryByText("Network")).not.toBeInTheDocument();
  });
});
