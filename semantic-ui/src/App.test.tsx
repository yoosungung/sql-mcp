import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "./App";
import type { AuthorApi } from "./api";
import type { Doc } from "./model";

vi.mock("./components/JoinCanvas", () => ({
  JoinCanvas: ({ view }: { view: Doc }) => (
    <div aria-label="Join canvas">join:{view.name}:{(view.sources || []).map((s) => s.table).join(",")}</div>
  ),
}));

function api(docs?: Doc[]): AuthorApi {
  const list = docs ?? [
    {
      kind: "table",
      name: "account",
      target: "bird",
      description: "Accounts",
      table: "account",
      schema: "main",
      columns: [{ name: "id", type: "int", nullable: false }],
      pk: ["id"],
    },
  ];
  return {
    listDatabases: async () => [{ name: "bird", engine: "sqlite" }],
    listPhysicalTables: async () => [{ schema: "main", name: "raw_extra" }],
    describePhysicalTable: async () => ({
      columns: [{ name: "id", data_type: "int", nullable: false }],
    }),
    samplePhysicalColumn: async () => ({ values: [] }),
    listDocs: async () => list,
    create: async () => {},
    update: async () => {},
    remove: async () => {},
  } as unknown as AuthorApi;
}

describe("production shell", () => {
  it("renders Tables tab from meta tables", async () => {
    render(<App api={api()} />);
    expect((await screen.findByRole("banner")).textContent).toContain("Semantic");
    expect(screen.getByRole("tab", { name: /Tables/ })).toBeTruthy();
    expect(screen.getByRole("tab", { name: /Cubes/ })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Register table" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Delete table" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Detail" }).textContent).toContain("account");
  });

  it("collapses the catalog and the inspector", async () => {
    const user = userEvent.setup();
    render(<App api={api()} />);
    await screen.findByRole("tab", { name: /Tables/ });

    await user.click(screen.getByRole("button", { name: "Collapse catalog" }));
    expect(screen.queryByRole("tab", { name: /Tables/ })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Expand catalog" }));
    expect(screen.getByRole("tab", { name: /Tables/ })).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Collapse inspector" }));
    expect(screen.queryByText("Open a view to list columns.")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Expand inspector" }));
    expect(screen.getByText("Open a view to list columns.")).toBeTruthy();
  });

  it("shows cube header on the cubes tab", async () => {
    const user = userEvent.setup();
    render(<App api={api()} />);
    await user.click(await screen.findByRole("tab", { name: /Cubes/ }));
    expect(screen.getByRole("region", { name: "Cube header" })).toBeTruthy();
  });

  it("creates a union view from dropped tables", async () => {
    const created: { combine?: string; joins?: unknown; sources?: unknown }[] = [];
    const user = userEvent.setup();
    const columns = [
      { name: "id", type: "int", nullable: false },
      { name: "name", type: "text", nullable: true },
    ];
    render(
      <App
        api={
          {
            ...api([
              { kind: "table", name: "account", target: "bird", table: "account", columns, pk: ["id"] },
              { kind: "table", name: "order", target: "bird", table: "order", columns, pk: ["id"] },
            ]),
            create: async (doc: { combine?: string; joins?: unknown; sources?: unknown }) => {
              created.push(doc);
            },
          } as unknown as AuthorApi
        }
      />,
    );
    await screen.findByRole("button", { name: "account" });
    dropOnto(screen.getByRole("button", { name: "account" }), screen.getByRole("button", { name: "union" }));
    dropOnto(screen.getByRole("button", { name: "order" }), screen.getByRole("button", { name: "union" }));
    expect(screen.getByLabelText("union canvas").textContent).toContain("account");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(created.some((c) => c.combine === "union")).toBe(true);
  });

  it("guards table delete when a view references it", async () => {
    const user = userEvent.setup();
    const removed: string[] = [];
    render(
      <App
        api={
          {
            ...api([
              { kind: "table", name: "account", target: "bird", table: "account", columns: [{ name: "id", type: "int", nullable: false }], pk: ["id"] },
              {
                kind: "view",
                name: "accounts",
                target: "bird",
                combine: "join",
                sources: [{ kind: "table", table: "account" }],
                columns: [],
                joins: [],
              },
            ]),
            remove: async (kind: string, name: string) => {
              removed.push(`${kind}/${name}`);
            },
          } as unknown as AuthorApi
        }
      />,
    );
    await user.click(await screen.findByRole("button", { name: "account" }));
    await user.click(screen.getByRole("button", { name: "Delete table" }));
    expect(removed).toEqual([]);
    expect(screen.getByText(/used by view/i)).toBeTruthy();
  });

  it("deletes an open view with its cube", async () => {
    const removed: string[] = [];
    const user = userEvent.setup();
    render(
      <App
        api={
          {
            ...api([
              { kind: "table", name: "account", target: "bird", table: "account", columns: [{ name: "id", type: "int", nullable: false }], pk: ["id"] },
              {
                kind: "view",
                name: "accounts",
                target: "bird",
                combine: "join",
                description: "Accounts",
                sources: [{ kind: "table", table: "account" }],
                columns: [{ name: "id", mappings: [{ source: "account", column: "id" }] }],
                joins: [],
              },
              { kind: "cube", name: "accounts_cube", target: "bird", view: "accounts", description: "", dimensions: [], facts: [] },
              { kind: "metric", name: "rows", target: "bird", cube: "accounts_cube" },
            ]),
            remove: async (kind: string, name: string) => {
              removed.push(`${kind}/${name}`);
            },
          } as unknown as AuthorApi
        }
      />,
    );
    await user.click(await screen.findByRole("button", { name: "Open" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Open" }));
    await user.click(screen.getByRole("button", { name: "Delete view" }));
    await user.click(screen.getByRole("button", { name: "Confirm delete" }));
    expect(removed).toEqual(["metric/rows", "cube/accounts_cube", "view/accounts"]);
  });
});

function dropOnto(source: HTMLElement, target: HTMLElement) {
  const store: Record<string, string> = {};
  const dataTransfer = {
    effectAllowed: "copy",
    dropEffect: "copy",
    setData: (key: string, value: string) => {
      store[key] = value;
    },
    getData: (key: string) => store[key] ?? "",
  };
  fireEvent.dragStart(source, { dataTransfer });
  fireEvent.dragOver(target, { dataTransfer });
  fireEvent.drop(target, { dataTransfer });
}
