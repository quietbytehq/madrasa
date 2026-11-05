import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { discoverServers, probeServer, setMode } from "@/app/mode";
import { ModePage } from "./ModePage";

vi.mock("@/app/mode", async () => {
  const actual = await vi.importActual<typeof import("@/app/mode")>("@/app/mode");
  return {
    ...actual,
    setMode: vi.fn(async (config) => config),
    discoverServers: vi.fn(async () => []),
    probeServer: vi.fn(),
  };
});

const FP = "AB:CD".repeat(16).slice(0, 95);

beforeEach(() => {
  vi.mocked(setMode).mockClear();
  vi.mocked(discoverServers).mockResolvedValue([]);
  vi.mocked(probeServer).mockReset();
});

describe("ModePage", () => {
  it("saves standalone mode with one click", async () => {
    const onChosen = vi.fn();
    renderWithProviders(<ModePage onChosen={onChosen} />);
    await userEvent.click(screen.getByRole("button", { name: /Just this PC/ }));
    await userEvent.click(screen.getByRole("button", { name: "Use just this PC" }));
    await waitFor(() =>
      expect(setMode).toHaveBeenCalledWith({
        mode: "standalone",
        server_url: null,
        fingerprint: null,
        listen_port: 8471,
      }),
    );
    expect(onChosen).toHaveBeenCalled();
  });

  it("starts the server on the chosen port and shows the firewall note", async () => {
    const onChosen = vi.fn();
    renderWithProviders(<ModePage onChosen={onChosen} />);
    await userEvent.click(screen.getByRole("button", { name: /This PC is the school server/ }));
    expect(screen.getByText("Windows Firewall")).toBeInTheDocument();
    expect(screen.getByText(/netsh advfirewall/)).toBeInTheDocument();
    const port = screen.getByLabelText("Port");
    await userEvent.clear(port);
    await userEvent.type(port, "9000");
    await userEvent.click(screen.getByRole("button", { name: "Start as the school server" }));
    await waitFor(() =>
      expect(setMode).toHaveBeenCalledWith({
        mode: "server",
        server_url: null,
        fingerprint: null,
        listen_port: 9000,
      }),
    );
    expect(onChosen).toHaveBeenCalled();
  });

  it("finds servers on the network, pins the fingerprint and connects only after confirmation", async () => {
    vi.mocked(discoverServers).mockResolvedValue([
      {
        host: "192.168.1.10",
        url: "https://192.168.1.10:8471",
        info: { name: "Maple Ridge", version: "0.1.3", port: 8471, fingerprint: FP },
      },
    ]);
    vi.mocked(probeServer).mockResolvedValue({
      url: "https://192.168.1.10:8471",
      fingerprint: FP,
      version: "0.1.3",
      needs_setup: false,
    });
    const onChosen = vi.fn();
    renderWithProviders(<ModePage onChosen={onChosen} />);
    await userEvent.click(screen.getByRole("button", { name: /Connect to a school server/ }));
    await userEvent.click(await screen.findByRole("button", { name: /Maple Ridge/ }));
    expect(probeServer).toHaveBeenCalledWith("https://192.168.1.10:8471");
    expect(await screen.findByLabelText("Fingerprint")).toHaveTextContent(FP);
    const connect = screen.getByRole("button", { name: "Connect" });
    expect(connect).toBeDisabled();
    await userEvent.click(screen.getByRole("checkbox"));
    await userEvent.click(connect);
    await waitFor(() =>
      expect(setMode).toHaveBeenCalledWith({
        mode: "client",
        server_url: "https://192.168.1.10:8471",
        fingerprint: FP,
        listen_port: 8471,
      }),
    );
    expect(onChosen).toHaveBeenCalled();
  });

  it("checks a typed address when nothing is found and shows the error", async () => {
    vi.mocked(probeServer).mockRejectedValue(new Error("No Madrasa server answered at 10.0.0.5"));
    renderWithProviders(<ModePage onChosen={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: /Connect to a school server/ }));
    expect(await screen.findByText(/No server answered/)).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Server address"), "10.0.0.5");
    await userEvent.click(screen.getByRole("button", { name: "Check" }));
    expect(probeServer).toHaveBeenCalledWith("10.0.0.5");
    expect(await screen.findByText(/No Madrasa server answered/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Connect" })).not.toBeInTheDocument();
  });
});
