import { Box, Button, Checkbox, Dialog, Flex, IconButton, Select, Tabs, Text } from "@radix-ui/themes";
import { useState } from "react";
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
  physicalCandidates,
  onRegister,
  onDeleteTable,
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
  physicalCandidates: { schema: string; name: string }[];
  onRegister: (physical: { schema: string; name: string }) => Promise<void>;
  onDeleteTable: (name: string) => Promise<void>;
}) {
  const [registerOpen, setRegisterOpen] = useState(false);
  const [pickPhysical, setPickPhysical] = useState("");

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
              <Tabs.Trigger value="tables">Tables</Tabs.Trigger>
              <Tabs.Trigger value="cubes">Cubes</Tabs.Trigger>
            </Tabs.List>
          </Tabs.Root>
          {tab === "tables" ? (
            <Box mt="3">
              <Flex gap="2" mb="3">
                <Button
                  size="1"
                  variant="soft"
                  aria-label="Register table"
                  disabled={busy || !physicalCandidates.length}
                  onClick={() => {
                    setPickPhysical(physicalCandidates[0] ? `${physicalCandidates[0].schema}:${physicalCandidates[0].name}` : "");
                    setRegisterOpen(true);
                  }}
                >
                  Register
                </Button>
                <Button
                  size="1"
                  variant="soft"
                  color="red"
                  aria-label="Delete table"
                  disabled={busy || !selectedTable}
                  onClick={() => selectedTable && void onDeleteTable(selectedTable.name)}
                >
                  Delete
                </Button>
              </Flex>
              <Text size="1" color="gray" as="p" mb="2">Registered tables. Drag onto the canvas.</Text>
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
              <Dialog.Root open={registerOpen} onOpenChange={setRegisterOpen}>
                <Dialog.Content maxWidth="420px">
                  <Dialog.Title>Register table</Dialog.Title>
                  <Dialog.Description size="2" mb="3">
                    Read a physical table and create metadata (add only; no edit).
                  </Dialog.Description>
                  <Select.Root value={pickPhysical} onValueChange={setPickPhysical}>
                    <Select.Trigger aria-label="Physical table" />
                    <Select.Content>
                      {physicalCandidates.map((p) => (
                        <Select.Item key={`${p.schema}:${p.name}`} value={`${p.schema}:${p.name}`}>
                          {p.schema}.{p.name}
                        </Select.Item>
                      ))}
                    </Select.Content>
                  </Select.Root>
                  <Flex gap="3" mt="4" justify="end">
                    <Dialog.Close>
                      <Button variant="soft" color="gray">Cancel</Button>
                    </Dialog.Close>
                    <Button
                      disabled={!pickPhysical || busy}
                      onClick={() => {
                        const [schema, name] = pickPhysical.split(":");
                        void onRegister({ schema, name }).then(() => setRegisterOpen(false));
                      }}
                    >
                      Register
                    </Button>
                  </Flex>
                </Dialog.Content>
              </Dialog.Root>
            </Box>
          ) : (
            <Box mt="3">
              <Text size="1" color="gray" as="p" mb="2">Saved views. Check to build a cube choice stack.</Text>
              {views.map((v) => (
                <Flex key={v.name} align="center" gap="2" className={viewName === v.name ? "nav-item on" : "nav-item"}>
                  <Checkbox
                    size="1"
                    aria-label={`check ${v.name}`}
                    checked={checked.includes(v.name)}
                    onCheckedChange={() => onToggleCheck(v.name)}
                  />
                  <button type="button" className="nav-label" onClick={() => onSelectView(v.name)}>
                    {v.name}
                  </button>
                </Flex>
              ))}
              <Button mt="3" style={{ width: "100%" }} disabled={checked.length < 1 || busy} onClick={onCombine}>
                Add to cube stack
              </Button>
            </Box>
          )}
        </>
      )}
    </nav>
  );
}
