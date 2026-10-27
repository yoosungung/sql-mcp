import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { ApiError, type AuthorApi } from "./api";
import type { Doc } from "./model";

vi.mock("./components/JoinCanvas", () => ({
  JoinCanvas: ({
    view,
    onSelectJoin,
    onSelectTable,
    onChange,
  }: {
    view: Doc;
    onSelectJoin?: (index: number) => void;
    onSelectTable?: (name: string) => void;
    onChange?: (view: Doc) => void;
  }) => (
    <div aria-label="Join canvas">
      join:{view.name}:{(view.sources || []).map((s) => s.alias || s.table).join(",")}
      {(view.sources || []).map((s) => {
        const id = s.alias || s.table;
        return (
          <button key={id} type="button" onClick={() => onSelectTable?.(id)}>
            Select {id}
          </button>
        );
      })}
      {(view.joins || []).length > 0 && (
        <button type="button" onClick={() => onSelectJoin?.(0)}>Select join</button>
      )}
      {(view.sources || []).length >= 2 && !(view.joins || []).length && (
        <button
          type="button"
          onClick={() =>
            onChange?.({
              ...view,
              joins: [
                {
                  left: view.sources![0].alias || view.sources![0].table,
                  right: view.sources![1].alias || view.sources![1].table,
                  type: "inner",
                  on: [{ left_column: "id", right_column: "id" }],
                },
              ],
            })
          }
        >
          Connect join
        </button>
      )}
    </div>
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
    expect(screen.getByRole("combobox", { name: "Database" })).toBeTruthy();
    expect(screen.queryByRole("combobox", { name: "Target" })).toBeNull();
  });

  it("shows table detail as header, columns, and filters without a separate physical path", async () => {
    render(<App api={api()} />);
    const detail = await screen.findByRole("region", { name: "Detail" });
    const header = within(detail).getByRole("region", { name: "Table header" });
    expect(header.textContent).toMatch(/account/);
    expect(header.textContent).toMatch(/bird/);
    expect(within(header).getByText("Database")).toBeTruthy();
    expect(within(header).queryByText("Target")).toBeNull();
    expect(within(detail).getByRole("region", { name: "Table columns" }).textContent).toMatch(/id/);
    const filters = within(detail).getByRole("region", { name: "Table filters" });
    expect(filters.textContent).toMatch(/scope/i);
    expect(within(detail).queryByText("Physical")).toBeNull();
    const grid = detail.querySelector(".table-detail") as HTMLElement | null;
    expect(grid).toBeTruthy();
    expect(grid?.className).toContain("table-detail");
    const scope = within(filters).getByRole("textbox", { name: "Scope filter" });
    expect(scope.closest(".filter-row")?.textContent).toMatch(/Scope/);
    expect(within(filters).getByRole("button", { name: "Remove scope filter" }).textContent).toBe("×");
  });

  it("aligns table columns as name, type, nullable, and FK", async () => {
    const user = userEvent.setup();
    render(
      <App
        api={api([
          {
            kind: "table",
            name: "orders",
            target: "bird",
            table: "orders",
            columns: [
              { name: "id", type: "int", nullable: false },
              { name: "customer_id", type: "int", nullable: false },
              { name: "note", type: "text", nullable: true },
            ],
            pk: ["id"],
            fk: [{ columns: ["customer_id"], references: { table: "customer", columns: ["id"] } }],
          },
          {
            kind: "table",
            name: "customer",
            target: "bird",
            table: "customer",
            columns: [{ name: "id", type: "int", nullable: false }],
            pk: ["id"],
          },
        ])}
      />,
    );
    await user.click(await screen.findByRole("button", { name: "orders" }));
    const cols = screen.getByRole("region", { name: "Table columns" });
    expect(within(cols).getByRole("columnheader", { name: "Name" })).toBeTruthy();
    expect(within(cols).getByRole("columnheader", { name: "Type" })).toBeTruthy();
    expect(within(cols).getByRole("columnheader", { name: "Nullable" })).toBeTruthy();
    expect(within(cols).getByRole("columnheader", { name: "FK" })).toBeTruthy();
    expect(within(cols).getByText("customer_id")).toBeTruthy();
    expect(within(cols).getByText("customer.id")).toBeTruthy();
    expect(within(cols).queryByRole("combobox")).toBeNull();
    expect(within(cols).getByText("yes")).toBeTruthy();
    expect(within(cols).getAllByText("no").length).toBeGreaterThan(0);
  });

  it("saves edited table scope, default, and permission filters", async () => {
    const user = userEvent.setup();
    const updated: Doc[] = [];
    const base = api();
    render(
      <App
        api={{
          ...base,
          update: async (_kind, _name, doc) => {
            updated.push(doc);
          },
        } as AuthorApi}
      />,
    );
    const filters = await screen.findByRole("region", { name: "Table filters" });
    await user.type(within(filters).getByRole("textbox", { name: "Scope filter" }), "id > 0");
    await user.type(within(filters).getByRole("textbox", { name: "Default filter" }), "id IS NOT NULL");
    await user.click(within(filters).getByRole("button", { name: "Add permission filter" }));
    expect(within(filters).getByRole("textbox", { name: "Permission role" }).closest(".filter-role")).toBeTruthy();
    expect(within(filters).getByRole("textbox", { name: "Permission expression" }).closest(".filter-role")).toBeNull();
    await user.type(within(filters).getByRole("textbox", { name: "Permission role" }), "branch");
    await user.type(within(filters).getByRole("textbox", { name: "Permission expression" }), "region = :r");
    await user.click(within(filters).getByRole("button", { name: "Save filters" }));
    expect(updated[0]?.scope_filter).toEqual({ expression: "id > 0" });
    expect(updated[0]?.default_filter).toEqual({ expression: "id IS NOT NULL" });
    expect(updated[0]?.permission_filters).toEqual([{ role: "branch", expression: "region = :r" }]);
  });

  it("removes table scope, default, and permission filters", async () => {
    const user = userEvent.setup();
    const updated: Doc[] = [];
    const base = api([
      {
        kind: "table",
        name: "account",
        target: "bird",
        table: "account",
        columns: [{ name: "id", type: "int", nullable: false }],
        pk: ["id"],
        scope_filter: { expression: "id > 0" },
        default_filter: { expression: "id IS NOT NULL" },
        permission_filters: [{ role: "branch", expression: "region = :r" }],
      },
    ]);
    render(
      <App
        api={{
          ...base,
          update: async (_kind, _name, doc) => {
            updated.push(doc);
          },
        } as AuthorApi}
      />,
    );
    const filters = await screen.findByRole("region", { name: "Table filters" });
    expect(within(filters).getByRole("textbox", { name: "Permission role" })).toBeTruthy();
    await user.click(within(filters).getByRole("button", { name: "Remove scope filter" }));
    await user.click(within(filters).getByRole("button", { name: "Remove default filter" }));
    await user.click(within(filters).getByRole("button", { name: "Remove permission filter" }));
    expect(within(filters).queryByRole("textbox", { name: "Permission role" })).toBeNull();
    await user.click(within(filters).getByRole("button", { name: "Save filters" }));
    expect(updated[0]?.scope_filter).toBeNull();
    expect(updated[0]?.default_filter).toBeNull();
    expect(updated[0]?.permission_filters).toEqual([]);
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

  it("keeps catalog tabs and collapse on one header row", async () => {
    render(<App api={api()} />);
    const header = await screen.findByRole("toolbar", { name: "Catalog header" });
    expect(within(header).getByRole("tab", { name: /Tables/ })).toBeTruthy();
    expect(within(header).getByRole("tab", { name: /Cubes/ })).toBeTruthy();
    expect(within(header).getByRole("button", { name: "Collapse catalog" })).toBeTruthy();
  });

  it("adds and deletes a view column from the inspector", async () => {
    const user = userEvent.setup();
    render(<App api={api()} />);
    await screen.findByRole("button", { name: "account" });
    dropOnto(screen.getByRole("button", { name: "account" }), screen.getByRole("main", { name: "Canvas" }));
    const inspector = screen.getByRole("complementary", { name: "Inspector" });
    await user.click(within(inspector).getByRole("button", { name: "Add column" }));
    expect(within(inspector).getByText("col")).toBeTruthy();
    const detail = screen.getByRole("region", { name: "Detail" });
    const name = within(detail).getByRole("textbox", { name: "Column name" });
    await user.clear(name);
    await user.type(name, "display_name");
    expect(within(inspector).getByText("display_name")).toBeTruthy();
    await user.click(within(inspector).getByRole("button", { name: "Delete column" }));
    expect(within(inspector).queryByText("display_name")).toBeNull();
  });

  it("edits the selected view column mapping in the detail panel", async () => {
    const user = userEvent.setup();
    render(
      <App
        api={api([
          {
            kind: "table",
            name: "account",
            target: "bird",
            table: "account",
            columns: [
              { name: "id", type: "int", nullable: false },
              { name: "name", type: "text", nullable: true },
            ],
            pk: ["id"],
          },
        ])}
      />,
    );
    await screen.findByRole("button", { name: "account" });
    dropOnto(screen.getByRole("button", { name: "account" }), screen.getByRole("main", { name: "Canvas" }));
    const inspector = screen.getByRole("complementary", { name: "Inspector" });
    await user.click(within(inspector).getByText("id"));
    const detail = screen.getByRole("region", { name: "Detail" });
    expect(within(detail).getByRole("textbox", { name: "Column name" })).toBeTruthy();
    expect(
      within(detail)
        .getAllByRole("region")
        .map((el) => el.getAttribute("aria-label")),
    ).toEqual(expect.arrayContaining(["Column", "Column mappings"]));
    const col = within(detail).getByRole("region", { name: "Column" });
    const maps = within(detail).getByRole("region", { name: "Column mappings" });
    expect(col.compareDocumentPosition(maps) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(maps.querySelector(".column-map-fields")).toBeTruthy();
    await user.click(within(detail).getByRole("combobox", { name: "Map account" }));
    await user.click(await screen.findByRole("option", { name: "name" }));
    expect(within(detail).getByRole("combobox", { name: "Map account" }).textContent).toMatch(/name/);
    expect(within(detail).getByText("text")).toBeTruthy();
  });

  it("keeps inspector collapse and Columns on one header row", async () => {
    render(<App api={api()} />);
    const header = await screen.findByRole("toolbar", { name: "Inspector header" });
    expect(within(header).getByRole("button", { name: "Collapse inspector" })).toBeTruthy();
    expect(within(header).getByRole("heading", { name: "Columns" })).toBeTruthy();
    expect(header.className).toMatch(/pane-bar-wrap/);
  });

  it("sizes the detail panel collapsed, standard, and expanded", async () => {
    const user = userEvent.setup();
    render(<App api={api()} />);
    const header = await screen.findByRole("toolbar", { name: "Detail header" });
    expect(within(header).getByRole("button", { name: "Standard detail" }).getAttribute("aria-pressed")).toBe("true");
    expect(document.querySelector(".detail-body")).toBeTruthy();

    await user.click(within(header).getByRole("button", { name: "Collapse detail" }));
    expect(document.querySelector(".shell.detail-collapsed")).toBeTruthy();
    expect(document.querySelector(".detail-body")).toBeNull();

    await user.click(screen.getByRole("button", { name: "Expand detail" }));
    expect(document.querySelector(".shell.detail-expanded")).toBeTruthy();
    expect(document.querySelector(".detail-body")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Expand detail" }).getAttribute("aria-pressed")).toBe("true");

    await user.click(screen.getByRole("button", { name: "Standard detail" }));
    expect(document.querySelector(".shell.detail-standard")).toBeTruthy();
  });

  it("closes the New view dialog on Cancel", async () => {
    const user = userEvent.setup();
    render(<App api={api()} />);
    await user.click(await screen.findByRole("button", { name: "New" }));
    expect(screen.getByRole("heading", { name: "New view" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("heading", { name: "New view" })).toBeNull();
  });

  it("creates a new view with the name typed in the dialog", async () => {
    const user = userEvent.setup();
    render(<App api={api()} />);
    await user.click(await screen.findByRole("button", { name: "New" }));
    const name = screen.getByRole("textbox", { name: "New view name" });
    await user.clear(name);
    await user.type(name, "hero_join");
    await user.click(screen.getByRole("button", { name: "Create" }));
    expect(screen.queryByRole("heading", { name: "New view" })).toBeNull();
    expect(screen.getByRole("heading", { name: "hero_join" })).toBeTruthy();
  });

  it("shows cube header on the cubes tab", async () => {
    const user = userEvent.setup();
    render(<App api={api()} />);
    await user.click(await screen.findByRole("tab", { name: /Cubes/ }));
    expect(screen.getByRole("region", { name: "Cube header" })).toBeTruthy();
  });

  it("does not duplicate join/union/minus under the view name", async () => {
    render(<App api={api()} />);
    await screen.findByRole("button", { name: "account" });
    dropOnto(screen.getByRole("button", { name: "account" }), screen.getByRole("main", { name: "Canvas" }));
    expect(screen.getByLabelText("View type")).toBeTruthy();
    expect(screen.getByText("Unsaved")).toBeTruthy();
    expect(screen.queryByRole("group", { name: "Combine" })).toBeNull();
    expect(screen.queryByRole("button", { name: "union" })).toBeNull();
  });

  it("creates a join view by dropping a table on the canvas", async () => {
    render(<App api={api()} />);
    await screen.findByRole("button", { name: "account" });
    dropOnto(screen.getByRole("button", { name: "account" }), screen.getByRole("main", { name: "Canvas" }));
    expect(screen.getByLabelText("Join canvas").textContent).toContain("account");
    expect(within(screen.getByRole("complementary", { name: "Inspector" })).queryByText("join_account")).toBeNull();
  });

  it("adds a second table dropped on an open join canvas", async () => {
    const columns = [
      { name: "id", type: "int", nullable: false },
      { name: "name", type: "text", nullable: true },
    ];
    render(
      <App
        api={api([
          { kind: "table", name: "account", target: "bird", table: "account", columns, pk: ["id"] },
          { kind: "table", name: "order", target: "bird", table: "order", columns, pk: ["id"] },
        ])}
      />,
    );
    await screen.findByRole("button", { name: "account" });
    dropOnto(screen.getByRole("button", { name: "account" }), screen.getByRole("main", { name: "Canvas" }));
    dropOnto(screen.getByRole("button", { name: "order" }), screen.getByRole("main", { name: "Canvas" }));
    expect(screen.getByLabelText("Join canvas").textContent).toContain("account");
    expect(screen.getByLabelText("Join canvas").textContent).toContain("order");
    expect(screen.queryByRole("button", { name: "Select join" })).toBeNull();
    expect(screen.getByRole("button", { name: "Connect join" })).toBeTruthy();
  });

  it("adds a second instance of the same table with a view source alias", async () => {
    render(<App api={api()} />);
    await screen.findByRole("button", { name: "account" });
    const canvas = screen.getByRole("main", { name: "Canvas" });
    dropOnto(screen.getByRole("button", { name: "account" }), canvas);
    dropOnto(screen.getByRole("button", { name: "account" }), canvas);
    expect(screen.getByLabelText("Join canvas").textContent).toMatch(/account,account_2/);
  });

  it("edits a join-view source alias in the table detail panel", async () => {
    const user = userEvent.setup();
    render(<App api={api()} />);
    await screen.findByRole("button", { name: "account" });
    const canvas = screen.getByRole("main", { name: "Canvas" });
    dropOnto(screen.getByRole("button", { name: "account" }), canvas);
    dropOnto(screen.getByRole("button", { name: "account" }), canvas);
    await user.click(screen.getByRole("button", { name: "Select account_2" }));
    const alias = screen.getByRole("textbox", { name: "Source alias" });
    await user.clear(alias);
    await user.type(alias, "bill_to");
    expect(screen.getByLabelText("Join canvas").textContent).toMatch(/bill_to/);
  });

  it("connects a join using source aliases of the same table", async () => {
    const user = userEvent.setup();
    render(<App api={api()} />);
    await screen.findByRole("button", { name: "account" });
    const canvas = screen.getByRole("main", { name: "Canvas" });
    dropOnto(screen.getByRole("button", { name: "account" }), canvas);
    dropOnto(screen.getByRole("button", { name: "account" }), canvas);
    await user.click(screen.getByRole("button", { name: "Connect join" }));
    await user.click(screen.getByRole("button", { name: "Select join" }));
    const detail = screen.getByRole("region", { name: "Detail" });
    expect(within(detail).getByRole("combobox", { name: "Join start" }).textContent).toMatch(/account/);
    expect(within(detail).getByRole("combobox", { name: "Join end" }).textContent).toMatch(/account_2/);
  });

  it("keeps existing joins when another table is dropped on the canvas", async () => {
    const user = userEvent.setup();
    const columns = [{ name: "id", type: "int", nullable: false }];
    render(
      <App
        api={api([
          { kind: "table", name: "account", target: "bird", table: "account", columns, pk: ["id"] },
          { kind: "table", name: "order", target: "bird", table: "order", columns, pk: ["id"] },
          { kind: "table", name: "item", target: "bird", table: "item", columns, pk: ["id"] },
        ])}
      />,
    );
    await screen.findByRole("button", { name: "account" });
    dropOnto(screen.getByRole("button", { name: "account" }), screen.getByRole("main", { name: "Canvas" }));
    dropOnto(screen.getByRole("button", { name: "order" }), screen.getByRole("main", { name: "Canvas" }));
    await user.click(screen.getByRole("button", { name: "Connect join" }));
    expect(screen.getByRole("button", { name: "Select join" })).toBeTruthy();
    dropOnto(screen.getByRole("button", { name: "item" }), screen.getByRole("main", { name: "Canvas" }));
    expect(screen.getByLabelText("Join canvas").textContent).toContain("item");
    expect(screen.getByRole("button", { name: "Select join" })).toBeTruthy();
  });

  it("edits join start, end, and columns in the detail panel", async () => {
    const user = userEvent.setup();
    const columns = [
      { name: "id", type: "int", nullable: false },
      { name: "name", type: "text", nullable: true },
    ];
    const orderCols = [
      { name: "id", type: "int", nullable: false },
      { name: "account_id", type: "int", nullable: false },
    ];
    render(
      <App
        api={api([
          { kind: "table", name: "account", target: "bird", table: "account", columns, pk: ["id"] },
          { kind: "table", name: "order", target: "bird", table: "order", columns: orderCols, pk: ["id"] },
        ])}
      />,
    );
    await screen.findByRole("button", { name: "account" });
    dropOnto(screen.getByRole("button", { name: "account" }), screen.getByRole("main", { name: "Canvas" }));
    dropOnto(screen.getByRole("button", { name: "order" }), screen.getByRole("main", { name: "Canvas" }));
    await user.click(screen.getByRole("button", { name: "Connect join" }));
    await user.click(screen.getByRole("button", { name: "Select join" }));
    const detail = screen.getByRole("region", { name: "Detail" });
    const startRow = within(detail).getByRole("group", { name: "Join start row" });
    const optionsRow = within(detail).getByRole("group", { name: "Join options row" });
    const endRow = within(detail).getByRole("group", { name: "Join end row" });
    expect(within(startRow).getByRole("combobox", { name: "Join start" })).toBeTruthy();
    expect(within(startRow).getByRole("combobox", { name: "Join start column" })).toBeTruthy();
    expect(within(optionsRow).getByRole("combobox", { name: "Join operator" })).toBeTruthy();
    expect(within(optionsRow).getByRole("combobox", { name: "Join type" })).toBeTruthy();
    expect(within(optionsRow).getByRole("combobox", { name: "Join relationship" })).toBeTruthy();
    expect(within(endRow).getByRole("combobox", { name: "Join end" })).toBeTruthy();
    expect(within(endRow).getByRole("combobox", { name: "Join end column" })).toBeTruthy();
    expect(
      within(detail)
        .getAllByRole("combobox")
        .map((el) => el.getAttribute("aria-label")),
    ).toEqual([
      "Join start",
      "Join start column",
      "Join operator",
      "Join type",
      "Join relationship",
      "Join end",
      "Join end column",
    ]);
    expect(within(detail).getByRole("combobox", { name: "Join start" }).textContent).toMatch(/account/i);
    expect(within(detail).getByRole("combobox", { name: "Join end" }).textContent).toMatch(/order/i);
    expect(within(detail).getByRole("combobox", { name: "Join start column" }).textContent).toMatch(/id/i);
    expect(within(detail).getByRole("combobox", { name: "Join operator" }).textContent).toMatch(/=/);
    expect(within(detail).getByRole("combobox", { name: "Join end column" }).textContent).toMatch(/id/i);
    expect(within(detail).getByRole("combobox", { name: "Join type" }).textContent).toMatch(/inner/i);
    expect(within(detail).queryByRole("button", { name: "inner" })).toBeNull();
    await user.click(within(detail).getByRole("combobox", { name: "Join operator" }));
    await user.click(await screen.findByRole("option", { name: ">" }));
    expect(within(detail).getByRole("combobox", { name: "Join operator" }).textContent).toMatch(/>/);
    await user.click(within(detail).getByRole("combobox", { name: "Join type" }));
    await user.click(await screen.findByRole("option", { name: "left" }));
    expect(within(detail).getByRole("combobox", { name: "Join type" }).textContent).toMatch(/left/i);
    await user.click(within(detail).getByRole("combobox", { name: "Join end column" }));
    await user.click(await screen.findByRole("option", { name: "account_id" }));
    expect(within(detail).getByRole("combobox", { name: "Join end column" }).textContent).toMatch(/account_id/i);
  });

  it("lets the user set an optional join relationship from unset", async () => {
    const user = userEvent.setup();
    const columns = [{ name: "id", type: "int", nullable: false }];
    render(
      <App
        api={api([
          { kind: "table", name: "account", target: "bird", table: "account", columns, pk: ["id"] },
          { kind: "table", name: "order", target: "bird", table: "order", columns, pk: ["id"] },
        ])}
      />,
    );
    await screen.findByRole("button", { name: "account" });
    dropOnto(screen.getByRole("button", { name: "account" }), screen.getByRole("main", { name: "Canvas" }));
    dropOnto(screen.getByRole("button", { name: "order" }), screen.getByRole("main", { name: "Canvas" }));
    await user.click(screen.getByRole("button", { name: "Connect join" }));
    await user.click(screen.getByRole("button", { name: "Select join" }));
    const detail = screen.getByRole("region", { name: "Detail" });
    expect(within(detail).getByRole("combobox", { name: "Join relationship" }).textContent).toMatch(/unset/i);
    await user.click(within(detail).getByRole("combobox", { name: "Join relationship" }));
    await user.click(await screen.findByRole("option", { name: "many_to_one" }));
    expect(within(detail).getByRole("combobox", { name: "Join relationship" }).textContent).toMatch(/many_to_one/i);
  });

  it("changes combine only from the header view type selector", async () => {
    const user = userEvent.setup();
    render(<App api={api()} />);
    await screen.findByRole("button", { name: "account" });
    dropOnto(screen.getByRole("button", { name: "account" }), screen.getByRole("main", { name: "Canvas" }));
    expect(screen.getByLabelText("View type").textContent).toMatch(/join/i);
    await user.click(screen.getByLabelText("View type"));
    await user.click(await screen.findByRole("option", { name: "union" }));
    expect(screen.queryByRole("button", { name: "union" })).toBeNull();
    expect(screen.getByLabelText("union canvas")).toBeTruthy();
    expect(screen.getByLabelText("View type").textContent).toMatch(/union/i);
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
    const userType = userEvent.setup();
    await userType.click(screen.getByLabelText("View type"));
    await userType.click(await screen.findByRole("option", { name: "union" }));
    dropOnto(screen.getByRole("button", { name: "account" }), screen.getByRole("main", { name: "Canvas" }));
    dropOnto(screen.getByRole("button", { name: "order" }), screen.getByRole("main", { name: "Canvas" }));
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

  it("adds a metric and a segment on an open cube", async () => {
    const created: { kind?: string; cube?: string; expression?: string }[] = [];
    const user = userEvent.setup();
    const columns = [{ name: "id", type: "int", nullable: false }];
    render(
      <App
        api={
          {
            ...api([
              { kind: "table", name: "account", target: "bird", table: "account", columns, pk: ["id"] },
              {
                kind: "view",
                name: "accounts",
                target: "bird",
                combine: "join",
                description: "",
                sources: [{ kind: "table", table: "account" }],
                columns: [{ name: "id", mappings: [{ source: "account", column: "id" }] }],
                joins: [],
              },
              { kind: "cube", name: "accounts_cube", target: "bird", view: "accounts", description: "", dimensions: [], facts: [] },
            ]),
            create: async (doc: { kind?: string; cube?: string; expression?: string }) => {
              created.push(doc);
            },
          } as unknown as AuthorApi
        }
      />,
    );
    await user.click(await screen.findByRole("tab", { name: /Cubes/ }));
    await user.click(screen.getByRole("button", { name: "accounts" }));
    await user.click(screen.getByRole("button", { name: "Add metric" }));
    expect(screen.getByRole("heading", { name: /metric · / })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Add segment" }));
    expect(screen.getByRole("heading", { name: /segment · / })).toBeTruthy();
    await user.click(within(screen.getByRole("region", { name: "Cube header" })).getByRole("button", { name: "Save" }));
    expect(created.some((c) => c.kind === "metric")).toBe(true);
    expect(created.some((c) => c.kind === "segment")).toBe(true);
  });

  it("shows API code and message and can dismiss the banner", async () => {
    const user = userEvent.setup();
    render(
      <App
        api={
          {
            ...api(),
            create: async () => {
              throw new ApiError("validation", "view columns required", 400);
            },
          } as unknown as AuthorApi
        }
      />,
    );
    await user.click(await screen.findByRole("button", { name: "New" }));
    await user.type(screen.getByRole("textbox", { name: "New view name" }), "draft");
    await user.click(screen.getByRole("button", { name: "Create" }));
    await user.click(screen.getByRole("button", { name: "Save" }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toMatch(/validation/);
    expect(alert.textContent).toMatch(/view columns required/);
    await user.click(screen.getByRole("button", { name: "Dismiss error" }));
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("deletes the previous view after a rename save", async () => {
    const created: string[] = [];
    const removed: string[] = [];
    const user = userEvent.setup();
    const docs: Doc[] = [
      { kind: "table", name: "account", target: "bird", table: "account", columns: [{ name: "id", type: "int", nullable: false }], pk: ["id"] },
      {
        kind: "view",
        name: "accounts",
        target: "bird",
        combine: "join",
        description: "",
        sources: [{ kind: "table", table: "account" }],
        columns: [{ name: "id", mappings: [{ source: "account", column: "id" }] }],
        joins: [],
      },
    ];
    render(
      <App
        api={
          {
            ...api(docs),
            create: async (doc: Doc) => {
              created.push(`${doc.kind}/${doc.name}`);
            },
            remove: async (kind: string, name: string) => {
              removed.push(`${kind}/${name}`);
            },
          } as unknown as AuthorApi
        }
      />,
    );
    await user.click(await screen.findByRole("button", { name: "Open" }));
    const openDialog = await screen.findByRole("dialog");
    await user.click(within(openDialog).getByRole("button", { name: "Open" }));
    await user.click(screen.getByRole("button", { name: "Rename" }));
    const renameDialog = await screen.findByRole("dialog");
    const rename = within(renameDialog).getByRole("textbox", { name: "Rename view name" });
    await user.clear(rename);
    await user.type(rename, "heroes");
    await user.click(within(renameDialog).getByRole("button", { name: "Rename" }));
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(created).toEqual(["view/heroes"]);
    expect(removed).toEqual(["view/accounts"]);
  });

  it("deletes an open view without deleting its cube and drops it from the stack", async () => {
    const removed: string[] = [];
    const updated: { kind?: string; view?: string; choice?: unknown }[] = [];
    let list: Doc[] = [
      { kind: "table", name: "account", target: "bird", table: "account", columns: [{ name: "id", type: "int", nullable: false }], pk: ["id"] },
      {
        kind: "view",
        name: "loans",
        target: "bird",
        combine: "join",
        description: "",
        sources: [{ kind: "table", table: "account" }],
        columns: [{ name: "id", mappings: [{ source: "account", column: "id" }] }],
        joins: [],
      },
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
      {
        kind: "cube",
        name: "accounts_cube",
        target: "bird",
        view: "accounts",
        description: "",
        dimensions: [],
        facts: [],
        choice: [{ view: "loans", summary: true }],
      },
      { kind: "metric", name: "rows", target: "bird", cube: "accounts_cube" },
    ];
    const user = userEvent.setup();
    render(
      <App
        api={
          {
            ...api(list),
            listDocs: async () => list,
            update: async (_kind: string, _name: string, doc: Doc) => {
              updated.push(doc);
              list = list.map((d) => (d.kind === doc.kind && d.name === _name ? { ...d, ...doc } : d));
            },
            remove: async (kind: string, name: string) => {
              removed.push(`${kind}/${name}`);
              list = list.filter((d) => !(d.kind === kind && d.name === name));
            },
          } as unknown as AuthorApi
        }
      />,
    );
    await user.click(await screen.findByRole("button", { name: "Open" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("combobox", { name: "Open view name" }));
    await user.click(await screen.findByRole("option", { name: "accounts" }));
    await user.click(within(dialog).getByRole("button", { name: "Open" }));
    await user.click(screen.getByRole("button", { name: "Delete view" }));
    await user.click(screen.getByRole("button", { name: "Confirm delete" }));
    expect(removed).toEqual(["view/accounts"]);
    expect(updated.some((c) => c.kind === "cube" && c.view === "loans")).toBe(true);
    await user.click(screen.getByRole("tab", { name: /Cubes/ }));
    expect(screen.getByRole("heading", { name: "accounts_cube" })).toBeTruthy();
    expect(screen.getByLabelText("Choice stack").textContent).toContain("loans");
    expect(screen.getByLabelText("Choice stack").textContent).not.toContain("accounts");
  });

  it("registers multiple physical tables from the register checkbox list", async () => {
    const created: string[] = [];
    const user = userEvent.setup();
    render(
      <App
        api={
          {
            ...api([]),
            listPhysicalTables: async () => [
              { schema: "main", name: "hero" },
              { schema: "main", name: "publisher" },
            ],
            create: async (doc: Doc) => {
              created.push(doc.name);
            },
          } as unknown as AuthorApi
        }
      />,
    );
    await user.click(await screen.findByRole("button", { name: "Register table" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("checkbox", { name: "register main.hero" }));
    await user.click(within(dialog).getByRole("checkbox", { name: "register main.publisher" }));
    await user.click(within(dialog).getByRole("button", { name: "Register" }));
    expect(created).toEqual(["hero", "publisher"]);
  });

  it("updates an already-registered table when it is checked again", async () => {
    const updated: string[] = [];
    const created: string[] = [];
    const user = userEvent.setup();
    render(
      <App
        api={
          {
            ...api([
              {
                kind: "table",
                name: "account",
                target: "bird",
                table: "account",
                columns: [{ name: "id", type: "int", nullable: false }],
                pk: ["id"],
              },
            ]),
            listPhysicalTables: async () => [
              { schema: "main", name: "account" },
              { schema: "main", name: "raw_extra" },
            ],
            create: async (doc: Doc) => {
              created.push(doc.name);
            },
            update: async (_kind: string, name: string) => {
              updated.push(name);
            },
          } as unknown as AuthorApi
        }
      />,
    );
    await user.click(await screen.findByRole("button", { name: "Register table" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("checkbox", { name: "register main.account" }).getAttribute("aria-checked")).toBe(
      "true",
    );
    expect(within(dialog).getByRole("checkbox", { name: "register main.raw_extra" }).getAttribute("aria-checked")).toBe(
      "false",
    );
    await user.click(within(dialog).getByRole("button", { name: "Register" }));
    expect(updated).toEqual(["account"]);
    expect(created).toEqual([]);
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
