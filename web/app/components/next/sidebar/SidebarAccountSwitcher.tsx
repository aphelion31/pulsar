import { Check, ChevronDown } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import type { Account } from "~/lib/api";

export type SidebarAccountSwitcherProps = {
  currentAccount?: Account;
  accounts?: Account[];
  isCollapsed?: boolean;
};

export function Logo({ className = "size-4" }: { className?: string }) {
  return (
    <img
      src="/app/brand-assets/logo_thumbnail.svg"
      alt="Pulsar"
      className={className}
    />
  );
}

export default function SidebarAccountSwitcher({
  currentAccount,
  accounts = [],
  isCollapsed = false,
}: SidebarAccountSwitcherProps) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const navigate = useNavigate();

  const showSwitcher = accounts.length > 1;

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (
        containerRef.current &&
        !containerRef.current.contains(event.target as Node)
      ) {
        setOpen(false);
      }
    }
    if (open) {
      document.addEventListener("mousedown", handleClickOutside);
      return () => document.removeEventListener("mousedown", handleClickOutside);
    }
  }, [open]);

  const handleSelect = (accountId: number) => {
    setOpen(false);
    navigate(`/accounts/${accountId}/conversations`);
  };

  return (
    <div ref={containerRef} className="relative min-w-0 flex-1">
      {isCollapsed ? (
        <button
          type="button"
          className={`grid shrink-0 place-content-center p-2 rounded-lg cursor-pointer hover:bg-n-alpha-1 transition-colors ${
            open ? "bg-n-alpha-1" : ""
          }`}
          title={currentAccount?.name}
          onClick={() => showSwitcher && setOpen(!open)}
        >
          <Logo className="size-7" />
        </button>
      ) : (
        <button
          type="button"
          id="sidebar-account-switcher"
          data-account-id={currentAccount?.id}
          className={`flex items-center gap-2 justify-between w-full rounded-lg px-2 py-1 text-left transition-colors ${
            open ? "bg-n-alpha-1" : ""
          } ${
            showSwitcher
              ? "hover:bg-n-alpha-1 cursor-pointer"
              : "cursor-default"
          }`}
          onClick={() => showSwitcher && setOpen(!open)}
        >
          <span className="text-sm font-medium leading-5 text-n-slate-12 truncate flex-1 min-w-0">
            {currentAccount?.name || "Pulsar"}
          </span>
          {showSwitcher && (
            <ChevronDown className="size-4 text-n-slate-10 shrink-0" />
          )}
        </button>
      )}

      {/* Switcher Dropdown */}
      {open && showSwitcher && (
        <div className="absolute top-full left-0 mt-1 z-50 min-w-72 rounded-xl bg-n-solid-1 shadow-lg border border-n-weak p-2">
          <div className="px-2 py-1.5 text-xs font-semibold uppercase tracking-wider text-n-slate-11">
            Switch account
          </div>
          <ul className="m-0 list-none mt-1">
            {accounts.map((acc) => {
              const isSelected = acc.id === currentAccount?.id;
              return (
                <li key={acc.id}>
                  <button
                    type="button"
                    className={`flex items-center justify-between w-full px-2 py-1.5 rounded-lg text-sm transition-colors text-left ${
                      isSelected
                        ? "bg-n-alpha-2 font-medium text-n-slate-12"
                        : "text-n-slate-11 hover:bg-n-alpha-2 hover:text-n-slate-12"
                    }`}
                    onClick={() => handleSelect(acc.id)}
                  >
                    <div className="flex items-center gap-2 min-w-0">
                      <span className="truncate max-w-36 text-n-slate-12">
                        {acc.name}
                      </span>
                      <div className="shrink-0 w-px h-3 bg-n-strong" />
                      <span className="text-xs text-n-slate-11 capitalize">
                        {acc.role}
                      </span>
                    </div>
                    {isSelected && (
                      <Check className="size-4 text-n-teal-11 shrink-0" />
                    )}
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </div>
  );
}
