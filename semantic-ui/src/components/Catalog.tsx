import { Box, Button, Checkbox, Flex, IconButton, Tabs, Text } from "@radix-ui/themes";
import type { Doc } from "../model";
import type { Tab } from "./types";

export function Catalog({
  open,
  onToggle,
  tab,
  onTab,
  tables,
  selectedTable,
  onSelectTable,
  picked,
  onTogglePick,
  views,
  viewName,
  checked,
  onToggleCheck,
  onSelectView,
  busy,
  onCombine,
}: {
  open: boolean;
  onToggle: () => void;
  tab: Tab;
  onTab: (tab: Tab) => void;
  tables: Doc[];
  selectedTable?: Doc;
  onSelectTable: (name: string) => void;
  picked: string[];
  onTogglePick: (name: string) => void;
  views: Doc[];
  viewName: string;
  checked: string[];
  onToggleCheck: (name: string) => void;
  onSelectView: (name: string) => void;
  busy: boolean;
  onCombine: () => void;
}) {
  return (
    <nav className={open ? "nav" : "nav is-collapsed"} aria-label="Catalog">
      <div className="pane-bar">
        <IconButton
          size="1"
          variant="ghost"
          color="gray"
          aria-expanded={open}
          aria-label={open ? "Collapse catalog" : "Expand catalog"}
          onClick={onToggle}
        >
          {open ? "‹" : "›"}
        </IconButton>
      </div>
      {open && (
        <>
          <Tabs.Root value={tab} onValueChange={(value) => onTab(value as Tab)}>
            <Tabs.List size="1">
              <Tabs.Trigger value="physical">Physical</Tabs.Trigger>
              <Tabs.Trigger value="cubes">Cubes</Tabs.Trigger>
            </Tabs.List>
          </Tabs.Root>
          {tab === "physical" ? (
            <Box mt="3">
              <Text size="1" color="gray" as="p" mb="2">Drag a table onto join, union, or minus.</Text>
              {tables.map((t) => (
                <Flex key={t.name} align="center" gap="2" className={selectedTable?.name === t.name ? "nav-item on" : "nav-item"}>
                  <Checkbox
                    size="1"
                    aria-label={t.name}
                    checked={picked.includes(t.name)}
                    onCheckedChange={() => onTogglePick(t.name)}
                  />
                  <button
                    type="button"
                    className="nav-label"
                    draggable
                    onDragStart={(event) => {
                      event.dataTransfer.setData("text/plain", t.name);
                      event.dataTransfer.effectAllowed = "copy";
                    }}
                    onClick={() => onSelectTable(t.name)}
                  >
                    {t.name}
                  </button>
                </Flex>
              ))}
            </Box>
          ) : (
            <Box mt="3">
              <Text size="1" color="gray" as="p" mb="2">Views, top to bottom.</Text>
              {views.map((v) => (
                <Flex key={v.name} align="center" gap="2" className={viewName === v.name ? "nav-item on" : "nav-item"}>
                  <Checkbox
                    size="1"
                    checked={checked.includes(v.name)}
                    onCheckedChange={() => onToggleCheck(v.name)}
                  />
                  <button type="button" className="nav-label" onClick={() => onSelectView(v.name)}>
                    {v.name}
                  </button>
                </Flex>
              ))}
              <Button mt="3" style={{ width: "100%" }} disabled={checked.length < 2 || busy} onClick={onCombine}>
                New view
              </Button>
              <Text size="1" color="gray" as="p" mt="2">The combined-view tree opens on the right.</Text>
            </Box>
          )}
        </>
      )}
    </nav>
  );
}
