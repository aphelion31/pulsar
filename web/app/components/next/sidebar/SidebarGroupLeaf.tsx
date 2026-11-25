import type { ReactNode } from "react";
import { Link } from "react-router";
import Icon from "~/components/next/Icon";
import SidebarUnreadBadge from "./SidebarUnreadBadge";

export type SidebarGroupLeafProps = {
  label: string;
  to?: string;
  icon?: string | ReactNode;
  active?: boolean;
  badgeCount?: number | string;
  hideTreeLine?: boolean;
  thinTreeLine?: boolean;
  onClick?: () => void;
};

export default function SidebarGroupLeaf({
  label,
  to = "",
  icon,
  active = false,
  badgeCount = 0,
  hideTreeLine = false,
  thinTreeLine = false,
  onClick,
}: SidebarGroupLeafProps) {
  const treeConnectorClass = hideTreeLine
    ? ""
    : [
        "child-item before:content-[''] before:absolute before:start-0 before:h-full before:bg-n-slate-4 first:before:rounded-t last:before:h-1/5",
        "last:after:content-[''] last:after:absolute last:after:start-0 last:after:bottom-[calc(50%_-_2px)] last:after:h-3 last:after:w-2.5 last:after:rounded-es last:after:border-n-slate-4",
        thinTreeLine
          ? "before:w-px last:after:border-b last:after:border-s"
          : "before:w-0.5 last:after:border-b-2 last:after:border-s-2",
      ].join(" ");

  const linkClass = [
    "flex h-8 items-center gap-2 px-2 py-1 rounded-lg transition-colors duration-100 ease-out group min-w-0 select-none",
    "ltr:hover:bg-gradient-to-r rtl:hover:bg-gradient-to-l from-transparent via-n-slate-3/70 to-n-slate-3/70",
    active
      ? "text-n-slate-12 bg-n-alpha-2 font-medium"
      : "text-n-slate-11 hover:text-n-slate-12",
  ].join(" ");

  const content = (
    <>
      {icon && (
        <span className="size-4 grid place-content-center shrink-0">
          {typeof icon === "string" ? (
            <Icon icon={icon} className="size-4 inline-block" />
          ) : (
            icon
          )}
        </span>
      )}
      <div className="flex-1 truncate min-w-0 text-sm leading-5">{label}</div>
      {Boolean(badgeCount) && <SidebarUnreadBadge count={badgeCount} />}
    </>
  );

  return (
    <li className={`relative py-0.5 ps-2 ms-3 min-w-0 ${treeConnectorClass}`}>
      {to ? (
        <Link to={to} className={linkClass} title={label} onClick={onClick}>
          {content}
        </Link>
      ) : (
        <div className={linkClass} title={label} onClick={onClick}>
          {content}
        </div>
      )}
    </li>
  );
}
