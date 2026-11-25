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
    <svg
      viewBox="0 0 32 32"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
    >
      <rect width="32" height="32" rx="8" fill="#2781F6" />
      <path
        d="M10.2 6.5H17.5A6.25 6.25 0 0 1 17.5 19H16.3A0.8 0.8 0 0 1 15.5 18.2V15.8A0.8 0.8 0 0 1 16.3 15H17.5A2.25 2.25 0 0 0 17.5 10.5H13.8A0.8 0.8 0 0 0 13 11.3V24.3A1.2 1.2 0 0 1 11.8 25.5H10.2A1.2 1.2 0 0 1 9 24.3V7.7A1.2 1.2 0 0 1 10.2 6.5Z"
        fill="#FFFFFF"
      />
    </svg>
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
