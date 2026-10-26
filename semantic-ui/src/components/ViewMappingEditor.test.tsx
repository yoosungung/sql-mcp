import { render, screen } from "@testing-library/react";
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
});
