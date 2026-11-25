import { useState } from "react";
import Button from "../components/next/Button";
import Avatar from "../components/next/Avatar";
import Icon from "../components/next/Icon";
import Input from "../components/next/Input";
import TextArea from "../components/next/TextArea";
import Spinner from "../components/next/Spinner";
import TabBar from "../components/next/TabBar";
import Switch from "../components/next/Switch";
import DropdownMenu from "../components/next/DropdownMenu";
import Label from "../components/next/Label";

export default function Components() {
  const [input, setInput] = useState("");
  const [text, setText] = useState("");
  const [enabled, setEnabled] = useState(false);
  const [tab, setTab] = useState(0);
  const [menu, setMenu] = useState(false);
  const tabs = [{ label: "All", count: 3 }, { label: "Open", icon: "i-lucide-inbox" }];
  return <main className="p-8 space-y-8 text-n-slate-12"><h1 className="text-2xl font-semibold">Components</h1>
    <section className="space-y-3"><h2 className="text-lg font-medium">Button</h2>{(["solid", "outline", "faded", "link", "ghost"] as const).map(variant => <div key={variant} className="flex flex-wrap gap-2">{(["blue", "ruby", "amber", "slate", "teal"] as const).map(color => <Button key={color} variant={variant} color={color} label={`${variant} ${color}`} />)}</div>)}<div className="flex flex-wrap items-center gap-2">{(["xs", "sm", "md", "lg"] as const).map(size => <Button key={size} size={size} label={size} />)}<Button label="Loading" isLoading /><Button label="Disabled" disabled /><Button icon="i-lucide-plus" aria-label="Add" /><Button label="Next" icon="i-lucide-arrow-right" trailingIcon /></div></section>
    <section className="space-y-3"><h2 className="text-lg font-medium">Avatar</h2><div className="flex flex-wrap items-center gap-3">{[16, 24, 32, 48, 64].map(size => <Avatar key={size} name="Alex Morgan" size={size} />)}<Avatar name="Alex Morgan" roundedFull status="online" /><Avatar name="Alex Morgan" status="busy" /><Avatar name="Alex Morgan" status="offline" /><Avatar name="" /></div></section>
    <section className="space-y-3"><h2 className="text-lg font-medium">Icon</h2><div className="flex gap-3"><Icon icon="i-lucide-search" /><Icon icon="i-lucide-inbox" /><Icon icon="i-woot-logic-or" className="size-6" /></div></section>
    <section className="space-y-3 max-w-md"><h2 className="text-lg font-medium">Input</h2><Input label="Input" modelValue={input} onModelValueChange={value => setInput(String(value))} placeholder="Type here" /><Input label="Small" size="sm" /><Input label="Error" messageType="error" message="Please check this value" /><Input label="Success" messageType="success" message="Looks good" /><Input label="Disabled" disabled /></section>
    <section className="space-y-3 max-w-md"><h2 className="text-lg font-medium">TextArea</h2><TextArea id="example-text" label="Message" modelValue={text} onModelValueChange={setText} showCharacterCount /><TextArea id="example-error" label="Error" messageType="error" message="Please check this value" /><TextArea id="example-disabled" label="Disabled" disabled /></section>
    <section className="space-y-3"><h2 className="text-lg font-medium">Spinner</h2><div className="flex items-center gap-3">{[16, 24, 32].map(size => <Spinner key={size} size={size} />)}</div></section>
    <section className="space-y-3"><h2 className="text-lg font-medium">TabBar</h2><TabBar tabs={tabs} initialActiveTab={tab} onTabChanged={value => setTab(tabs.findIndex(item => item.label === value.label))} /></section>
    <section className="space-y-3"><h2 className="text-lg font-medium">Switch</h2><div className="flex gap-3"><Switch aria-label="Toggle setting" modelValue={enabled} onModelValueChange={setEnabled} /><Switch aria-label="Enabled setting" modelValue /><Switch aria-label="Disabled setting" disabled /></div></section>
    <section className="space-y-3"><h2 className="text-lg font-medium">Label</h2><div className="flex flex-wrap gap-2">{(["slate", "amber", "teal", "ruby", "blue", "iris"] as const).map(color => <Label key={color} color={color} label={color} />)}<Label compact label={{ title: "Compact", color: "#2a8", description: "Custom color" }} /><Label label="Icon" icon={<Icon icon="i-lucide-tag" className="size-4" />} /></div></section>
    <section className="space-y-3 pb-48"><h2 className="text-lg font-medium">DropdownMenu</h2><div className="relative"><Button label="Toggle menu" onClick={() => setMenu(!menu)} />{menu && <DropdownMenu showSearch menuSections={[{ title: "Actions", items: [{ label: "Edit", action: "edit", icon: "i-lucide-pencil" }, { label: "Delete", action: "delete", icon: "i-lucide-trash" }, { label: "Disabled", disabled: true }] }]} onAction={() => setMenu(false)} />}</div></section>
  </main>;
}
