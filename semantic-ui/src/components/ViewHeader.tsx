import { useState } from "react";
import { Badge, Button, Dialog, Flex, Heading, Select, Text, TextField } from "@radix-ui/themes";
import type { TableCombine } from "./docs";

export function ViewHeader({
  title,
  combine,
  unsaved,
  busy,
  canSave,
  canDelete,
  views,
  onNew,
  onOpen,
  onRename,
  onSave,
  onDelete,
  onCombineChange,
}: {
  title: string;
  combine?: TableCombine;
  unsaved: boolean;
  busy: boolean;
  canSave: boolean;
  canDelete: boolean;
  views: { name: string }[];
  onNew: (combine: TableCombine, name: string) => void;
  onOpen: (name: string) => void;
  onRename: (name: string) => void;
  onSave: () => void;
  onDelete: () => void;
  onCombineChange?: (combine: TableCombine) => void;
}) {
  const [askDelete, setAskDelete] = useState(false);
  const [openPicker, setOpenPicker] = useState(false);
  const [renameOpen, setRenameOpen] = useState(false);
  const [newOpen, setNewOpen] = useState(false);
  const [draftName, setDraftName] = useState(title);
  const [newName, setNewName] = useState("");
  const [newCombine, setNewCombine] = useState<TableCombine>("join");
  const [openName, setOpenName] = useState(views[0]?.name || "");

  return (
    <Flex direction="column" gap="2" className="view-bar" aria-label="View header">
      <Flex align="center" justify="between" gap="3" wrap="wrap">
        <Flex align="center" gap="2">
          <Heading size="3" weight="medium">{title || "No view"}</Heading>
          {combine && (
            <Select.Root
              size="1"
              value={combine}
              onValueChange={(v) => onCombineChange?.(v as TableCombine)}
              disabled={!onCombineChange}
            >
              <Select.Trigger aria-label="View type" />
              <Select.Content>
                <Select.Item value="join">join</Select.Item>
                <Select.Item value="union">union</Select.Item>
                <Select.Item value="minus">minus</Select.Item>
              </Select.Content>
            </Select.Root>
          )}
          {unsaved && <Badge variant="soft">Unsaved</Badge>}
        </Flex>
        <Flex align="center" gap="2" wrap="wrap">
          <Button type="button" size="1" variant="soft" onClick={() => { setNewCombine("join"); setNewName(""); setNewOpen(true); }}>New</Button>
          <Button type="button" size="1" variant="soft" onClick={() => { setOpenName(views[0]?.name || ""); setOpenPicker(true); }}>Open</Button>
          <Button type="button" size="1" variant="soft" disabled={!title} onClick={() => { setDraftName(title); setRenameOpen(true); }}>Rename</Button>
          <Button type="button" size="1" disabled={!canSave || busy} onClick={onSave}>Save</Button>
          <Button type="button" size="1" variant="soft" color="red" disabled={!canDelete || busy} onClick={() => setAskDelete(true)} aria-label="Delete view">Delete</Button>
        </Flex>
      </Flex>
      {askDelete && (
        <Flex align="center" justify="between" gap="3" role="alertdialog" aria-label="Delete view">
          <Text size="2">Delete {title}? Cubes keep existing; this view is dropped from their stack.</Text>
          <Flex gap="2">
            <Button type="button" size="1" variant="soft" color="gray" onClick={() => setAskDelete(false)}>Cancel</Button>
            <Button type="button" size="1" color="red" aria-label="Confirm delete" onClick={() => { setAskDelete(false); onDelete(); }}>Delete</Button>
          </Flex>
        </Flex>
      )}

      <Dialog.Root open={newOpen} onOpenChange={setNewOpen}>
        <Dialog.Content maxWidth="360px">
          <Dialog.Title>New view</Dialog.Title>
          <Flex direction="column" gap="3" mt="3">
            <TextField.Root
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder="View name"
              aria-label="New view name"
            />
            <Select.Root value={newCombine} onValueChange={(v) => setNewCombine(v as TableCombine)}>
              <Select.Trigger aria-label="New view type" />
              <Select.Content>
                <Select.Item value="join">join</Select.Item>
                <Select.Item value="union">union</Select.Item>
                <Select.Item value="minus">minus</Select.Item>
              </Select.Content>
            </Select.Root>
          </Flex>
          <Flex gap="3" mt="4" justify="end">
            <Dialog.Close><Button variant="soft" color="gray">Cancel</Button></Dialog.Close>
            <Button disabled={!newName.trim()} onClick={() => { onNew(newCombine, newName.trim()); setNewOpen(false); }}>Create</Button>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>

      <Dialog.Root open={openPicker} onOpenChange={setOpenPicker}>
        <Dialog.Content maxWidth="360px">
          <Dialog.Title>Open view</Dialog.Title>
          {views.length ? (
            <Select.Root value={openName} onValueChange={setOpenName}>
              <Select.Trigger aria-label="Open view name" />
              <Select.Content>
                {views.map((v) => (
                  <Select.Item key={v.name} value={v.name}>{v.name}</Select.Item>
                ))}
              </Select.Content>
            </Select.Root>
          ) : (
            <Text size="2" color="gray">No saved views.</Text>
          )}
          <Flex gap="3" mt="4" justify="end">
            <Dialog.Close><Button variant="soft" color="gray">Cancel</Button></Dialog.Close>
            <Button disabled={!openName} onClick={() => { onOpen(openName); setOpenPicker(false); }}>Open</Button>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>

      <Dialog.Root open={renameOpen} onOpenChange={setRenameOpen}>
        <Dialog.Content maxWidth="360px">
          <Dialog.Title>Rename view</Dialog.Title>
          <TextField.Root value={draftName} onChange={(e) => setDraftName(e.target.value)} aria-label="Rename view name" />
          <Flex gap="3" mt="4" justify="end">
            <Dialog.Close><Button variant="soft" color="gray">Cancel</Button></Dialog.Close>
            <Button disabled={!draftName.trim()} onClick={() => { onRename(draftName.trim()); setRenameOpen(false); }}>Rename</Button>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>
    </Flex>
  );
}

export function CubeHeader({
  title,
  unsaved,
  busy,
  canSave,
  canDelete,
  onNew,
  onRename,
  onSave,
  onDelete,
}: {
  title: string;
  unsaved: boolean;
  busy: boolean;
  canSave: boolean;
  canDelete: boolean;
  onNew: () => void;
  onRename: (name: string) => void;
  onSave: () => void;
  onDelete: () => void;
}) {
  const [askDelete, setAskDelete] = useState(false);
  const [renameOpen, setRenameOpen] = useState(false);
  const [draftName, setDraftName] = useState(title);
  return (
    <div className="view-bar" role="region" aria-label="Cube header">
    <Flex direction="column" gap="2">
      <Flex align="center" justify="between" gap="3" wrap="wrap">
        <Flex align="center" gap="2">
          <Heading size="3" weight="medium">{title || "No cube"}</Heading>
          {unsaved && <Badge variant="soft">Unsaved</Badge>}
        </Flex>
        <Flex align="center" gap="2">
          <Button type="button" size="1" variant="soft" onClick={onNew}>New</Button>
          <Button type="button" size="1" variant="soft" disabled={!title} onClick={() => { setDraftName(title); setRenameOpen(true); }}>Rename</Button>
          <Button type="button" size="1" disabled={!canSave || busy} onClick={onSave}>Save</Button>
          <Button type="button" size="1" variant="soft" color="red" disabled={!canDelete || busy} onClick={() => setAskDelete(true)} aria-label="Delete cube">Delete</Button>
        </Flex>
      </Flex>
      {askDelete && (
        <Flex align="center" justify="between" gap="3" role="alertdialog" aria-label="Delete cube">
          <Text size="2">Delete cube {title}? Metrics and segments on it are removed.</Text>
          <Flex gap="2">
            <Button type="button" size="1" variant="soft" color="gray" onClick={() => setAskDelete(false)}>Cancel</Button>
            <Button type="button" size="1" color="red" aria-label="Confirm delete cube" onClick={() => { setAskDelete(false); onDelete(); }}>Delete</Button>
          </Flex>
        </Flex>
      )}
      <Dialog.Root open={renameOpen} onOpenChange={setRenameOpen}>
        <Dialog.Content maxWidth="360px">
          <Dialog.Title>Rename cube</Dialog.Title>
          <TextField.Root value={draftName} onChange={(e) => setDraftName(e.target.value)} aria-label="Rename cube name" />
          <Flex gap="3" mt="4" justify="end">
            <Dialog.Close><Button variant="soft" color="gray">Cancel</Button></Dialog.Close>
            <Button disabled={!draftName.trim()} onClick={() => { onRename(draftName.trim()); setRenameOpen(false); }}>Rename</Button>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>
    </Flex>
    </div>
  );
}
