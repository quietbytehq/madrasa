/**
 * How this PC takes part: on its own, as the school server on the LAN, or as a client of a
 * school server. The desktop shell owns the choice (it decides whether to start the database);
 * the browser build has no shell and behaves as standalone.
 */
export type Mode = "standalone" | "server" | "client";

export interface ModeConfig {
  mode: Mode;
  server_url: string | null;
  fingerprint: string | null;
  listen_port: number;
}

export interface NetworkInfo {
  mode: Mode;
  addresses: string[];
  port: number;
  fingerprint: string | null;
  server_url: string | null;
}

export interface FoundServer {
  host: string;
  url: string;
  info: { name: string; version: string; port: number; fingerprint: string };
}

export interface ServerProbe {
  url: string;
  fingerprint: string;
  version: string;
  needs_setup: boolean;
}

export const DEFAULT_PORT = 8471;

function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

/** The saved mode; `null` on a fresh desktop install; standalone outside the desktop shell. */
export async function getMode(): Promise<ModeConfig | null> {
  if (!inTauri()) {
    return { mode: "standalone", server_url: null, fingerprint: null, listen_port: DEFAULT_PORT };
  }
  return invoke<ModeConfig | null>("get_mode");
}

export async function setMode(config: ModeConfig): Promise<ModeConfig> {
  return invoke<ModeConfig>("set_mode", { config });
}

export async function networkInfo(): Promise<NetworkInfo | null> {
  if (!inTauri()) return null;
  return invoke<NetworkInfo | null>("network_info");
}

export async function discoverServers(): Promise<FoundServer[]> {
  return invoke<FoundServer[]>("discover_servers");
}

export async function probeServer(url: string): Promise<ServerProbe> {
  return invoke<ServerProbe>("probe_server", { url });
}

/** The Windows command that lets a school server accept LAN connections, for the note. */
export function firewallCommand(): string {
  return 'netsh advfirewall firewall add rule name="Madrasa" dir=in action=allow program="C:\\Program Files\\Madrasa\\Madrasa.exe" enable=yes profile=private';
}
