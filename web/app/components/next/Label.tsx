import type { ReactNode } from "react";

type Props = { label: string | { title: string; description?: string; color?: string }; compact?: boolean; color?: "slate" | "amber" | "teal" | "ruby" | "blue" | "iris"; icon?: ReactNode; action?: ReactNode };
const colors = {
  slate: "bg-n-label-color outline-n-label-border text-n-slate-12",
  amber: "bg-n-amber-2 outline-n-amber-4 text-n-amber-11",
  teal: "bg-n-teal-2 outline-n-teal-4 text-n-teal-11",
  ruby: "bg-n-ruby-2 outline-n-ruby-4 text-n-ruby-11",
  blue: "bg-n-blue-2 outline-n-blue-4 text-n-blue-11",
  iris: "bg-n-iris-2 outline-n-iris-4 text-n-iris-11",
};
export default function Label({ label, compact, color = "slate", icon, action }: Props) {
  const object = typeof label === "string" ? null : label;
  return <div title={object?.description || ""} className={`rounded-lg -outline-offset-1 outline outline-1 inline-flex items-center flex-shrink-0 ${colors[color]} ${compact ? "px-1.5 h-6 gap-1 rounded-md" : "px-2.5 h-8 gap-1.5 rounded-lg"}`}>
    {object?.color ? <span className={`rounded-sm flex-shrink-0 ${compact ? "size-1.5" : "size-2"}`} style={{ background: object.color }} /> : icon}
    <span className={`whitespace-nowrap ${compact ? "text-label-small" : "text-label !font-420"}`}>{object?.title ?? label as string}</span>{action}
  </div>;
}
