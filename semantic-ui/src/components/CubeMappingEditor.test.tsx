import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Theme } from "@radix-ui/themes";
import { describe, expect, it, vi } from "vitest";
import type { Doc } from "../model";
import { CubeMappingEditor } from "./CubeMappingEditor";

const view: Doc = {
  kind: "view",
  name: "heroes",
  columns: [{ name: "id", mappings: [{ source: "superhero", column: "id" }] }],
};

const cube: Doc = {
  kind: "cube",
  name: "heroes_cube",
  view: "heroes",
  description: "",
  dimensions: [],
  facts: [],
};

describe("CubeMappingEditor", () => {
  it("keeps an unmapped member checked after click", async () => {
    const user = userEvent.setup();
    render(
      <Theme>
        <CubeMappingEditor open onOpenChange={() => {}} cube={cube} views={[view]} onApply={vi.fn()} />
      </Theme>,
    );
    const box = await screen.findByRole("checkbox", { name: "map id" });
    expect(box.getAttribute("data-state")).toBe("unchecked");
    await user.click(box);
    expect(screen.getByRole("checkbox", { name: "map id" }).getAttribute("data-state")).toBe("checked");
  });

  it("applies schema member fields including encryption", async () => {
    const user = userEvent.setup();
    const onApply = vi.fn();
    render(
      <Theme>
        <CubeMappingEditor open onOpenChange={() => {}} cube={cube} views={[view]} onApply={onApply} />
      </Theme>,
    );
    await user.selectOptions(await screen.findByRole("combobox", { name: "kind id" }), "fact");
    await user.click(screen.getByRole("checkbox", { name: "map id" }));
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(onApply.mock.calls[0][0].facts[0]).toMatchObject({
      name: "id",
      type: "quantity",
      column: "id",
      encryption: false,
    });
    expect(onApply.mock.calls[0][0].dimensions).toEqual([]);
  });
});
