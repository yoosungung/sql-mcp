import { StrictMode } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Theme } from "@radix-ui/themes";
import { describe, expect, it, vi } from "vitest";
import type { Doc } from "../model";
import { ViewMappingEditor } from "./ViewMappingEditor";

const account: Doc = {
  kind: "table",
  name: "account",
  pk: ["id"],
  columns: [
    { name: "id", type: "int", nullable: false },
    { name: "name", type: "text", nullable: true },
  ],
};

describe("ViewMappingEditor", () => {
  it("shows source aliases as table column headers", () => {
    render(
      <Theme>
        <ViewMappingEditor
          open
          onOpenChange={() => {}}
          view={{
            kind: "view",
            name: "v",
            sources: [
              { kind: "table", table: "account" },
              { kind: "table", table: "account", alias: "bill_to" },
            ],
            columns: [{
              name: "id",
              mappings: [
                { source: "account", column: "id" },
                { source: "bill_to", column: "id" },
              ],
            }],
          }}
          tables={[account]}
          onApply={vi.fn()}
        />
      </Theme>,
    );
    const dialog = screen.getByRole("dialog", { name: "View mapping editor" });
    expect(dialog.textContent).toMatch(/bill_to/);
    expect(screen.getByLabelText("id → account")).toBeTruthy();
    expect(screen.getByLabelText("id → bill_to")).toBeTruthy();
  });

  it("lists union tables in the dragged sources order", () => {
    const order: Doc = {
      kind: "table",
      name: "order",
      pk: ["id"],
      columns: [{ name: "id", type: "int", nullable: false }],
    };
    render(
      <Theme>
        <ViewMappingEditor
          open
          onOpenChange={() => {}}
          view={{
            kind: "view",
            name: "v",
            combine: "union",
            sources: [
              { kind: "table", table: "order" },
              { kind: "table", table: "account" },
            ],
            columns: [{
              name: "id",
              mappings: [
                { source: "order", column: "id" },
                { source: "account", column: "id" },
              ],
            }],
          }}
          tables={[account, order]}
          onApply={vi.fn()}
        />
      </Theme>,
    );
    const left = screen.getByLabelText("id → order");
    const right = screen.getByLabelText("id → account");
    expect(left.compareDocumentPosition(right) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("keeps focus while renaming a mapped view column", async () => {
    const user = userEvent.setup();
    render(
      <Theme>
        <ViewMappingEditor
          open
          onOpenChange={() => {}}
          view={{
            kind: "view",
            name: "v",
            sources: [{ kind: "table", table: "account" }],
            columns: [{ name: "id", mappings: [{ source: "account", column: "id" }] }],
          }}
          tables={[account]}
          onApply={vi.fn()}
        />
      </Theme>,
    );
    expect(screen.queryByRole("button", { name: "split id" })).toBeNull();
    const input = screen.getByRole("textbox", { name: "name id" });
    await user.click(input);
    await user.keyboard("x");
    expect(document.activeElement).toBe(input);
    expect((input as HTMLInputElement).value).toBe("idx");
    await user.keyboard("2");
    expect(document.activeElement).toBe(input);
    expect((input as HTMLInputElement).value).toBe("idx2");
  });

  it("lists mapped columns by name then unmapped hints", () => {
    render(
      <Theme>
        <ViewMappingEditor
          open
          onOpenChange={() => {}}
          view={{
            kind: "view",
            name: "v",
            sources: [{ kind: "table", table: "account" }],
            columns: [
              { name: "zebra", mappings: [{ source: "account", column: "id" }] },
              { name: "apple", mappings: [{ source: "account", column: "name" }] },
            ],
          }}
          tables={[account]}
          onApply={vi.fn()}
        />
      </Theme>,
    );
    const apple = screen.getByRole("textbox", { name: "name apple" });
    const zebra = screen.getByRole("textbox", { name: "name zebra" });
    expect(apple.compareDocumentPosition(zebra) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("applies mapped columns in the original view order", async () => {
    const user = userEvent.setup();
    const onApply = vi.fn();
    render(
      <Theme>
        <ViewMappingEditor
          open
          onOpenChange={() => {}}
          view={{
            kind: "view",
            name: "v",
            sources: [{ kind: "table", table: "account" }],
            columns: [
              { name: "zebra", mappings: [{ source: "account", column: "id" }] },
              { name: "apple", mappings: [{ source: "account", column: "name" }] },
            ],
          }}
          tables={[account]}
          onApply={onApply}
        />
      </Theme>,
    );
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(onApply.mock.calls[0][0].columns.map((c: { name: string }) => c.name)).toEqual([
      "zebra",
      "apple",
    ]);
  });

  it("splits a source mapping into a new view column", async () => {
    const user = userEvent.setup();
    const onApply = vi.fn();
    render(
      <StrictMode>
        <Theme>
          <ViewMappingEditor
            open
            onOpenChange={() => {}}
            view={{
              kind: "view",
              name: "v",
              sources: [
                { kind: "table", table: "account" },
                { kind: "table", table: "account", alias: "bill_to" },
              ],
              columns: [{
                name: "id",
                mappings: [
                  { source: "account", column: "id" },
                  { source: "bill_to", column: "id" },
                ],
              }],
            }}
            tables={[account]}
            onApply={onApply}
          />
        </Theme>
      </StrictMode>,
    );
    const map = screen.getByLabelText("map id");
    const split = screen.getByRole("button", { name: "split id" });
    const name = screen.getByRole("textbox", { name: "name id" });
    expect(map.compareDocumentPosition(split) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(split.compareDocumentPosition(name) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.queryByRole("button", { name: "split id ← bill_to" })).toBeNull();
    await user.click(split);
    expect(screen.getAllByRole("textbox", { name: "name id_2" })).toHaveLength(1);
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(onApply.mock.calls[0][0].columns).toEqual([
      { name: "id", mappings: [{ source: "account", column: "id" }] },
      { name: "id_2", mappings: [{ source: "bill_to", column: "id" }] },
    ]);
  });

  it("does not re-sort while renaming, and marks duplicate active names", async () => {
    const user = userEvent.setup();
    render(
      <Theme>
        <ViewMappingEditor
          open
          onOpenChange={() => {}}
          view={{
            kind: "view",
            name: "v",
            sources: [{ kind: "table", table: "account" }],
            columns: [
              { name: "name", mappings: [{ source: "account", column: "name" }] },
              { name: "id", mappings: [{ source: "account", column: "id" }] },
            ],
          }}
          tables={[account]}
          onApply={vi.fn()}
        />
      </Theme>,
    );
    const id = screen.getByRole("textbox", { name: "name id" });
    const name = screen.getByRole("textbox", { name: "name name" });
    expect(id.compareDocumentPosition(name) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    await user.click(name);
    await user.keyboard("{backspace}{backspace}{backspace}{backspace}id");
    expect(document.activeElement).toBe(name);
    expect((name as HTMLInputElement).value).toBe("id");
    expect(id.compareDocumentPosition(name) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(id.getAttribute("aria-invalid")).toBe("true");
    expect(name.getAttribute("aria-invalid")).toBe("true");
  });
});
