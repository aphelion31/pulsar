import { ChevronDown, ChevronUp } from "lucide-react";
import { useState } from "react";
import type { ReactNode } from "react";
import Icon from "~/components/next/Icon";
import SidebarGroupLeaf from "./SidebarGroupLeaf";

export type SidebarSubGroupChild = {
  name: string;
  label: string;
  to: string;
  icon?: string | ReactNode;
  badgeCount?: number | string;
  exact?: boolean;
};

export type SidebarSubGroupProps = {
  name: string;
  label: string;
  icon?: string | ReactNode;
  children?: SidebarSubGroupChild[];
  collapsible?: boolean;
  isExpanded?: boolean;
  showTreeLine?: boolean;
  endTreeLine?: boolean;
  activePath?: string;
};

export default function SidebarSubGroup({
  name: _name,
  label,
  icon,
  children = [],
  collapsible = true,
  isExpanded = true,
  showTreeLine = false,
  endTreeLine = false,
  activePath = "",
}: SidebarSubGroupProps) {
  const [isSubExpanded, setIsSubExpanded] = useState(true);

  if (!children.length) return null;

  const treeVertical =
    "before:content-[''] before:absolute before:-top-1 before:w-0.5 before:bg-n-slate-4 before:start-[-0.5rem]";
  const treeElbow =
    "after:content-[''] after:absolute after:w-2.5 after:h-3 after:bottom-1/2 after:start-[-0.5rem] after:border-b-2 after:border-s-2 after:rounded-es after:border-n-slate-4";
  const childrenTrunk =
    "before:content-[''] before:absolute before:top-0 before:bottom-0 before:w-0.5 before:bg-n-slate-4 before:start-[-0.5rem]";

  return (
    <li className="relative flex flex-col list-none min-w-0">
      {/* Subgroup header separator */}
      {isExpanded && (
        <div className={`relative min-w-0 my-1 ${collapsible ? "ms-5" : ""}`}>
          <button
            type="button"
            className={[
              "relative flex h-8 w-full min-w-0 items-center justify-between gap-2 rounded-lg px-2 py-1.5 text-n-slate-10 select-none cursor-pointer hover:bg-n-alpha-2 transition-colors",
              showTreeLine && treeVertical,
              showTreeLine && (endTreeLine ? `before:h-3 ${treeElbow}` : "before:-bottom-1"),
            ]
              .filter(Boolean)
              .join(" ")}
            onClick={() => setIsSubExpanded(!isSubExpanded)}
          >
            <div className="inline-flex min-w-0 items-center gap-2">
              {icon && (
                typeof icon === "string" ? (
                  <Icon icon={icon} className="size-4 shrink-0" />
                ) : (
                  icon
                )
              )}
              <span className="flex-grow truncate text-start text-sm font-medium leading-5">
                {label}
              </span>
            </div>
            {collapsible && (
              <span className="size-4 flex items-center justify-center text-n-slate-10">
                {isSubExpanded ? (
                  <ChevronUp className="size-3" />
                ) : (
                  <ChevronDown className="size-3" />
                )}
              </span>
            )}
          </button>
        </div>
      )}

      {/* Children list */}
      {isSubExpanded && (
        <ul
          className={[
            "m-0 list-none relative min-w-0",
            collapsible ? "ms-5" : "",
            showTreeLine && !endTreeLine ? childrenTrunk : "",
          ]
            .filter(Boolean)
            .join(" ")}
        >
          <div className="min-w-0 max-h-60 overflow-y-auto no-scrollbar">
            {children.map((child, index) => {
              const isActive = child.exact
                ? activePath === child.to
                : activePath === child.to ||
                  (child.to !== "/" && activePath.startsWith(`${child.to}/`));
              const isLast = index === children.length - 1;
              return (
                <SidebarGroupLeaf
                  key={child.name}
                  label={child.label}
                  to={child.to}
                  icon={child.icon}
                  badgeCount={child.badgeCount}
                  active={isActive}
                  hideTreeLine={!isExpanded}
                  thinTreeLine={isLast}
                />
              );
            })}
          </div>
        </ul>
      )}
    </li>
  );
}
