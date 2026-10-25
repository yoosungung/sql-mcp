import { toolPayload, type Doc } from "./model";

type Rpc = {
  error?: { message?: string };
  result?: { isError?: boolean; content?: { text?: string }[] };
};

export class McpApi {
  private id = 1;
  private ready: Promise<void> | null = null;

  constructor(private readonly url = "/mcp") {}

  private async post(method: string, params?: unknown, note = false): Promise<Rpc> {
    const body: Record<string, unknown> = { jsonrpc: "2.0", method };
    if (!note) body.id = this.id++;
    if (params !== undefined) body.params = params;
    const res = await fetch(this.url, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json, text/event-stream",
      },
      body: JSON.stringify(body),
    });
    if (note) return {};
    const text = await res.text();
    if (!res.ok) throw new Error(text || `HTTP ${res.status}`);
    return JSON.parse(text) as Rpc;
  }

  async init(): Promise<void> {
    if (!this.ready) {
      this.ready = (async () => {
        await this.post("initialize", {
          protocolVersion: "2026-07-28",
          capabilities: {},
          clientInfo: { name: "semantic-ui", version: "0.1.0" },
        });
        await this.post("notifications/initialized", undefined, true);
      })();
    }
    await this.ready;
  }

  private async call(name: string, args: Record<string, unknown>): Promise<unknown> {
    await this.init();
    const rpc = await this.post("tools/call", { name, arguments: args });
    return toolPayload(rpc);
  }

  async listDatabases(): Promise<{ name: string; description?: string; engine?: string }[]> {
    const data = (await this.call("list_databases", {})) as { items?: { name: string }[] };
    return data.items ?? [];
  }

  async listTables(database: string): Promise<{ database: string; name: string; description: string }[]> {
    const data = (await this.call("list_tables", { database })) as {
      items?: { database: string; name: string; description: string }[];
    };
    return data.items ?? [];
  }

  async listDocs(kind?: string): Promise<Doc[]> {
    const data = (await this.call("metadata_list", kind ? { kind } : {})) as { items?: Doc[] };
    return data.items ?? [];
  }

  async create(body: Doc): Promise<void> {
    await this.call("metadata_create", { body });
  }

  async update(kind: string, name: string, body: Doc): Promise<void> {
    await this.call("metadata_update", { kind, name, body });
  }

  async remove(kind: string, name: string): Promise<void> {
    await this.call("metadata_delete", { kind, name });
  }
}
