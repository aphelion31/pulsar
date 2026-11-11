import { ChevronDown, ChevronUp } from "lucide-react";
import type { MouseEvent, ReactNode } from "react";
import { Link } from "react-router";
import Icon from "~/components/next/Icon";
import SidebarUnreadBadge from "./SidebarUnreadBadge";

export type SidebarGroupHeaderProps = {
  to?: string;
  label: string;
  icon?: string | ReactNode;
  expandable?: boolean;
  isExpanded?: boolean;
  isActive?: boolean;
  hasActiveChild?: boolean;
  badgeCount?: number | string;
  onToggle?: () => void;
};

export default function SidebarGroupHeader({
  to,
  label,
  icon,
  expandable = false,
  isExpanded = false,
  isActive = false,
  hasActiveChild = false,
  badgeCount = 0,
  onToggle,
}: SidebarGroupHeaderProps) {
  const handleClick = (e: MouseEvent) => {
    if (e.metaKey || e.altKey || e.ctrlKey || e.shiftKey) return;
    if (onToggle) {
      e.preventDefault();
      onToggle();
    }
  };

  const className = [
    "flex items-center gap-2 px-1.5 py-1 rounded-lg h-8 min-w-0 select-none cursor-pointer text-sm transition-colors duration-100 ease-out",
    isActive && !hasActiveChild
      ? "text-n-slate-12 bg-n-alpha-2 font-medium"
      : hasActiveChild
        ? "text-n-slate-12 font-medium"
        : "text-n-slate-11 hover:bg-n-alpha-2",
  ]
    .filter(Boolean)
    .join(" ");

  const content = (
    <>
      {icon && (
        <div className="relative flex items-center gap-2 shrink-0">
          {typeof icon === "string" ? (
            <Icon icon={icon} className="size-4 shrink-0" />
          ) : (
            icon
          )}
        </div>
      )}
      <div className="flex items-center gap-1.5 flex-1 min-w-0 justify-between">
        <span
          className={`truncate ${
            isActive || hasActiveChild ? "font-medium text-sm text-n-slate-12" : "text-sm text-n-slate-11"
          }`}
        >
          {label}
        </span>
        {Boolean(badgeCount) && !expandable && (
          <SidebarUnreadBadge count={badgeCount} />
        )}
      </div>
      {expandable && (
        <span
          className="shrink-0 p-0.5 text-n-slate-10 hover:text-n-slate-12"
          onClick={(e) => {
            e.stopPropagation();
            e.preventDefault();
            onToggle?.();
          }}
        >
          {isExpanded ? (
            <ChevronUp className="size-3" />
          ) : (
            <ChevronDown className="size-3" />
          )}
        </span>
      )}
    </>
  );

  if (to) {
    return (
      <Link to={to} className={className} title={label} onClick={handleClick}>
        {content}
      </Link>
    );
  }

  return (
    <div
      role="button"
      tabIndex={0}
      className={className}
      title={label}
      onClick={handleClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onToggle?.();
        }
      }}
    >
      {content}
    </div>
  );
}
