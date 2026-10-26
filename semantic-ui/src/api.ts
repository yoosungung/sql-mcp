import { type Doc } from "./model";

export type Database = { name: string; description?: string; engine?: string };
export type PhysicalTable = { schema: string; name: string };
export type PhysicalColumn = { name: string; data_type?: string; nullable?: boolean };

function apiError(data: { message?: string; code?: string }, text: string, status: number): Error {
  return new Error(data.message || data.code || text || `HTTP ${status}`);
}

export class AuthorApi {
  constructor(private readonly base = "/api") {}

  private async json(path: string, init?: RequestInit): Promise<unknown> {
    const res = await fetch(`${this.base}${path}`, {
      ...init,
      headers: { "Content-Type": "application/json", ...(init?.headers as Record<string, string>) },
    });
    const text = await res.text();
    const data = text ? (JSON.parse(text) as { message?: string; code?: string }) : {};
    if (!res.ok) throw apiError(data, text, res.status);
    return data;
  }

  async listDatabases(): Promise<Database[]> {
    const data = (await this.json("/databases")) as { items?: Database[] };
    return data.items ?? [];
  }

  async listPhysicalTables(database: string): Promise<PhysicalTable[]> {
    const data = (await this.json(`/databases/${encodeURIComponent(database)}/tables`)) as {
      items?: PhysicalTable[];
    };
    return data.items ?? [];
  }

  async describePhysicalTable(
    database: string,
    table: string,
    schema?: string,
  ): Promise<{ columns: PhysicalColumn[] }> {
    const q = schema ? `?schema=${encodeURIComponent(schema)}` : "";
    return (await this.json(
      `/databases/${encodeURIComponent(database)}/tables/${encodeURIComponent(table)}${q}`,
    )) as { columns: PhysicalColumn[] };
  }

  async samplePhysicalColumn(
    database: string,
    table: string,
    column: string,
    schema?: string,
  ): Promise<{ values: unknown[] }> {
    const params = new URLSearchParams();
    if (schema) params.set("schema", schema);
    const q = params.toString() ? `?${params}` : "";
    return (await this.json(
      `/databases/${encodeURIComponent(database)}/tables/${encodeURIComponent(table)}/columns/${encodeURIComponent(column)}/sample${q}`,
    )) as { values: unknown[] };
  }

  async listDocs(kind?: string): Promise<Doc[]> {
    const q = kind ? `?kind=${encodeURIComponent(kind)}` : "";
    const data = (await this.json(`/meta${q}`)) as { items?: Doc[] };
    return data.items ?? [];
  }

  async create(body: Doc): Promise<void> {
    await this.json("/meta", { method: "POST", body: JSON.stringify(body) });
  }

  async update(kind: string, name: string, body: Doc): Promise<void> {
    await this.json(`/meta/${encodeURIComponent(kind)}/${encodeURIComponent(name)}`, {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }

  async remove(kind: string, name: string): Promise<void> {
    await this.json(`/meta/${encodeURIComponent(kind)}/${encodeURIComponent(name)}`, {
      method: "DELETE",
    });
  }
}
