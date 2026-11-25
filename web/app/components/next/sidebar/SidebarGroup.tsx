import { useState } from "react";
import type { ReactNode } from "react";
import { Link } from "react-router";
import Icon from "~/components/next/Icon";
import SidebarGroupHeader from "./SidebarGroupHeader";
import SidebarGroupLeaf from "./SidebarGroupLeaf";
import SidebarSubGroup from "./SidebarSubGroup";
import type { SidebarSubGroupChild } from "./SidebarSubGroup";

export type SidebarChildItem = {
  name: string;
  label: string;
  to?: string;
  icon?: string | ReactNode;
  badgeCount?: number | string;
  collapsible?: boolean;
  showTreeLine?: boolean;
  exact?: boolean;
  children?: SidebarSubGroupChild[];
};

export type SidebarGroupProps = {
  name: string;
  label: string;
  icon?: string | ReactNode;
  to?: string;
  children?: SidebarChildItem[];
  badgeCount?: number | string;
  isCollapsed?: boolean;
  isExpanded?: boolean;
  onToggle?: () => void;
  activePath?: string;
};

function matchesPath(activePath: string, to?: string, exact?: boolean): boolean {
  if (!to) return false;
  if (exact) return activePath === to;
  return activePath === to || (to !== "/" && activePath.startsWith(`${to}/`));
}

export default function SidebarGroup({
  name: _name,
  label,
  icon,
  to,
  children,
  badgeCount = 0,
  isCollapsed = false,
  isExpanded = false,
  onToggle,
  activePath = "",
}: SidebarGroupProps) {
  const [hovered, setHovered] = useState(false);

  const hasChildren = Boolean(children && children.length > 0);

  // Check if any child matches current path
  const hasActiveChild = hasChildren
    ? children?.some((child) => {
        if (matchesPath(activePath, child.to, child.exact)) {
          return true;
        }
        if (child.children) {
          return child.children.some((sub) =>
            matchesPath(activePath, sub.to, sub.exact)
          );
        }
        return false;
      })
    : false;

  const isActive = matchesPath(activePath, to);

  const firstLink = to || children?.find((c) => c.to)?.to || children?.[0]?.children?.[0]?.to;

  // Collapsed state
  if (isCollapsed) {
    const buttonClass = [
      "relative flex items-center justify-center size-10 rounded-lg transition-colors duration-100 ease-out select-none",
      isActive || hasActiveChild
        ? "text-n-slate-12 bg-n-alpha-2 font-medium"
        : "text-n-slate-11 hover:bg-n-alpha-2 hover:text-n-slate-12",
    ].join(" ");

    return (
      <li
        className="relative grid place-content-center text-sm"
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
      >
        {firstLink ? (
          <Link to={firstLink} className={buttonClass} title={label}>
            {typeof icon === "string" ? (
              <Icon icon={icon} className="size-4 shrink-0" />
            ) : (
              icon
            )}
          </Link>
        ) : (
          <button type="button" className={buttonClass} title={label} onClick={onToggle}>
            {typeof icon === "string" ? (
              <Icon icon={icon} className="size-4 shrink-0" />
            ) : (
              icon
            )}
          </button>
        )}

        {/* Collapsed flyout popover on hover if group has children */}
        {hovered && hasChildren && (
          <div className="absolute left-full top-0 ml-2 z-50 w-56 rounded-xl bg-n-solid-1 shadow-lg border border-n-weak p-2">
            <div className="px-2 py-1.5 text-xs font-semibold uppercase tracking-wider text-n-slate-11 border-b border-n-weak mb-1">
              {label}
            </div>
            <ul className="m-0 list-none max-h-80 overflow-y-auto no-scrollbar">
              {children?.map((child) => {
                if (child.children) {
                  return (
                    <li key={child.name} className="py-1">
                      <div className="px-2 py-1 text-xs font-medium text-n-slate-10">
                        {child.label}
                      </div>
                      <ul className="m-0 list-none ps-2">
                        {child.children.map((sub) => (
                          <li key={sub.name} className="py-0.5">
                            <Link
                              to={sub.to}
                              className={`flex items-center gap-2 px-2 py-1 rounded-md text-xs transition-colors ${
                                activePath === sub.to
                                  ? "bg-n-alpha-2 text-n-slate-12 font-medium"
                                  : "text-n-slate-11 hover:bg-n-alpha-2 hover:text-n-slate-12"
                              }`}
                            >
                              <span className="truncate flex-1">{sub.label}</span>
                            </Link>
                          </li>
                        ))}
                      </ul>
                    </li>
                  );
                }
                return (
                  <li key={child.name} className="py-0.5">
                    <Link
                      to={child.to || "#"}
                      className={`flex items-center gap-2 px-2 py-1 rounded-md text-xs transition-colors ${
                        activePath === child.to
                          ? "bg-n-alpha-2 text-n-slate-12 font-medium"
                          : "text-n-slate-11 hover:bg-n-alpha-2 hover:text-n-slate-12"
                      }`}
                    >
                      <span className="truncate flex-1">{child.label}</span>
                    </Link>
                  </li>
                );
              })}
            </ul>
          </div>
        )}
      </li>
    );
  }

  // Expanded state
  return (
    <li className="grid gap-1 text-sm select-none min-w-0">
      <SidebarGroupHeader
        icon={icon}
        label={label}
        to={to}
        expandable={hasChildren}
        isExpanded={isExpanded}
        isActive={isActive}
        hasActiveChild={hasActiveChild}
        badgeCount={badgeCount}
        onToggle={onToggle}
      />
      {hasChildren && (isExpanded || hasActiveChild) && (
        <ul className="grid m-0 list-none min-w-0">
          {children?.map((child) => {
            if (child.children) {
              return (
                <SidebarSubGroup
                  key={child.name}
                  name={child.name}
                  label={child.label}
                  icon={child.icon}
                  children={child.children}
                  collapsible={child.collapsible}
                  showTreeLine={child.showTreeLine}
                  isExpanded={isExpanded}
                  activePath={activePath}
                />
              );
            }

            const childActive = matchesPath(activePath, child.to, child.exact);

            return (
              <SidebarGroupLeaf
                key={child.name}
                label={child.label}
                to={child.to}
                icon={child.icon}
                badgeCount={child.badgeCount}
                active={childActive}
                hideTreeLine={false}
              />
            );
          })}
        </ul>
      )}
    </li>
  );
}
