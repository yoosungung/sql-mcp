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
    expect(box.getAttribute("data-state")).toBe("checked");
  });
});
