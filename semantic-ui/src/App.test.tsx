import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "./App";
import type { McpApi } from "./api";

function api(): McpApi {
  return {
    listDatabases: async () => [{ name: "bird", engine: "sqlite" }],
    listTables: async () => [],
    listDocs: async () => [{ kind: "table", name: "account", target: "bird", description: "Accounts" }],
    create: async () => {},
    update: async () => {},
    remove: async () => {},
  } as unknown as McpApi;
}

describe("production shell", () => {
  it("renders an indigo slate workspace", async () => {
    render(<App api={api()} />);
    const banner = await screen.findByRole("banner");
    expect(banner.textContent).toContain("Semantic");
    expect(document.querySelector('[data-accent-color="indigo"]')).not.toBeNull();
    expect(document.querySelector('[data-gray-color="slate"]')).not.toBeNull();
    expect(screen.getByRole("navigation", { name: "Catalog" })).toBeTruthy();
    expect(screen.getByRole("main", { name: "Canvas" })).toBeTruthy();
    expect(screen.getByRole("complementary", { name: "Inspector" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Detail" }).textContent).toContain("account");
    expect(screen.getByRole("tab", { name: /Physical/ })).toBeTruthy();
    expect(screen.getByRole("tab", { name: /Cubes/ })).toBeTruthy();
  });

  it("collapses the catalog and the inspector", async () => {
    const user = userEvent.setup();
    render(<App api={api()} />);
    await screen.findByRole("tab", { name: /Physical/ });

    await user.click(screen.getByRole("button", { name: "Collapse catalog" }));
    expect(screen.queryByRole("tab", { name: /Physical/ })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Expand catalog" }));
    expect(screen.getByRole("tab", { name: /Physical/ })).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Collapse inspector" }));
    expect(screen.queryByText("Check tables to map columns.")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Expand inspector" }));
    expect(screen.getByText("Check tables to map columns.")).toBeTruthy();
  });

  it("keeps combine on the cubes tab", async () => {
    const user = userEvent.setup();
    render(<App api={api()} />);
    await screen.findByRole("tab", { name: /Cubes/ });
    await user.click(screen.getByRole("tab", { name: /Cubes/ }));
    expect(screen.getByRole("button", { name: "New view" })).toBeTruthy();
  });

  it("creates a union view from checked tables", async () => {
    const created: { combine?: string; joins?: unknown; sources?: unknown }[] = [];
    const user = userEvent.setup();
    const columns = [{ name: "id" }, { name: "name" }];
    render(
      <App
        api={
          {
            ...api(),
            listDocs: async () => [
              { kind: "table", name: "account", target: "bird", columns },
              { kind: "table", name: "order", target: "bird", columns },
            ],
            create: async (doc: { combine?: string; joins?: unknown; sources?: unknown }) => {
              created.push(doc);
            },
          } as unknown as McpApi
        }
      />,
    );
    await user.click(await screen.findByRole("checkbox", { name: "account" }));
    await user.click(screen.getByRole("button", { name: "union" }));
    expect((screen.getByRole("button", { name: "Save view" }) as HTMLButtonElement).disabled).toBe(true);
    await user.click(screen.getByRole("checkbox", { name: "order" }));
    await user.click(screen.getByRole("button", { name: "Save view" }));
    expect(created).toEqual([
      expect.objectContaining({
        combine: "union",
        sources: [
          { kind: "table", table: "account" },
          { kind: "table", table: "order" },
        ],
      }),
    ]);
    expect(created[0].joins).toBeUndefined();
  });

  it("drops physical tables onto union", async () => {
    const columns = [{ name: "id" }, { name: "name" }];
    render(
      <App
        api={
          {
            ...api(),
            listDocs: async () => [
              { kind: "table", name: "account", target: "bird", columns },
              { kind: "table", name: "order", target: "bird", columns },
            ],
          } as unknown as McpApi
        }
      />,
    );
    const account = await screen.findByRole("button", { name: "account" });
    dropOnto(account, screen.getByRole("button", { name: "union" }));
    expect(screen.getByRole("button", { name: "union" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("checkbox", { name: "account" }).getAttribute("aria-checked")).toBe("true");
    expect((screen.getByRole("button", { name: "Save view" }) as HTMLButtonElement).disabled).toBe(true);

    dropOnto(screen.getByRole("button", { name: "order" }), screen.getByRole("button", { name: "union" }));
    expect(screen.getByRole("main", { name: "Canvas" }).textContent).toContain("account");
    expect(screen.getByRole("main", { name: "Canvas" }).textContent).toContain("order");
    expect((screen.getByRole("button", { name: "Save view" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("deletes an open view with its cube", async () => {
    const removed: string[] = [];
    const user = userEvent.setup();
    render(
      <App
        api={
          {
            ...api(),
            listDocs: async () => [
              { kind: "table", name: "account", target: "bird" },
              { kind: "view", name: "accounts", target: "bird", combine: "select", description: "Accounts", sources: [], columns: [] },
              { kind: "cube", name: "accounts_cube", target: "bird", view: "accounts" },
              { kind: "metric", name: "rows", target: "bird", cube: "accounts_cube" },
            ],
            remove: async (kind: string, name: string) => {
              removed.push(`${kind}/${name}`);
            },
          } as unknown as McpApi
        }
      />,
    );
    await user.click(await screen.findByRole("tab", { name: /Cubes/ }));
    await user.click(screen.getByRole("button", { name: "accounts" }));
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
