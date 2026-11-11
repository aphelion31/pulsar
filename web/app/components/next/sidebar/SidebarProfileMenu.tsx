import { LogOut, Palette, Settings, User } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Link, useNavigate } from "react-router";
import Avatar from "~/components/next/Avatar";
import { signOut, type Profile } from "~/lib/api";

export type SidebarProfileMenuProps = {
  me: Profile;
  accountId: number;
  isCollapsed?: boolean;
};

export default function SidebarProfileMenu({
  me,
  accountId,
  isCollapsed = false,
}: SidebarProfileMenuProps) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const navigate = useNavigate();

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

  const handleLogout = async () => {
    setOpen(false);
    await signOut();
    navigate("/login", { replace: true });
  };

  return (
    <div ref={containerRef} className="relative min-w-0 w-full">
      <button
        type="button"
        className={`flex items-center gap-2 p-1 text-left rounded-lg cursor-pointer hover:bg-n-alpha-1 transition-colors ${
          open ? "bg-n-alpha-1" : ""
        } ${isCollapsed ? "justify-center w-auto mx-auto" : "w-full"}`}
        title={isCollapsed ? me.available_name : undefined}
        onClick={() => setOpen(!open)}
      >
        <Avatar
          size={32}
          name={me.available_name}
          src={me.avatar_url || undefined}
          status="online"
        />
        {!isCollapsed && (
          <div className="min-w-0 flex-1">
            <div className="text-sm font-medium leading-4 truncate text-n-slate-12">
              {me.available_name}
            </div>
            <div className="text-xs truncate text-n-slate-11">
              {me.email}
            </div>
          </div>
        )}
      </button>

      {/* Profile menu dropdown */}
      {open && (
        <div className="absolute bottom-full left-0 mb-2 z-50 w-72 rounded-xl bg-n-solid-1 shadow-lg border border-n-weak p-2">
          {/* Availability Status */}
          <div className="flex items-center justify-between px-2 py-1.5 border-b border-n-weak mb-1">
            <span className="text-xs font-medium text-n-slate-11">Availability</span>
            <span className="inline-flex items-center gap-1.5 text-xs text-n-teal-11 font-medium bg-n-teal-3 px-2 py-0.5 rounded-full">
              <span className="size-2 rounded-full bg-n-teal-9" />
              Online
            </span>
          </div>

          <ul className="m-0 list-none">
            <li>
              <Link
                to={`/accounts/${accountId}/profile/settings`}
                className="flex items-center gap-2.5 px-2 py-1.5 rounded-lg text-sm text-n-slate-11 hover:bg-n-alpha-2 hover:text-n-slate-12 transition-colors"
                onClick={() => setOpen(false)}
              >
                <User className="size-4 shrink-0" />
                <span>Profile settings</span>
              </Link>
            </li>
            <li>
              <Link
                to={`/accounts/${accountId}/settings`}
                className="flex items-center gap-2.5 px-2 py-1.5 rounded-lg text-sm text-n-slate-11 hover:bg-n-alpha-2 hover:text-n-slate-12 transition-colors"
                onClick={() => setOpen(false)}
              >
                <Settings className="size-4 shrink-0" />
                <span>Settings</span>
              </Link>
            </li>
            <li>
              <button
                type="button"
                className="flex items-center gap-2.5 w-full px-2 py-1.5 rounded-lg text-sm text-n-slate-11 hover:bg-n-alpha-2 hover:text-n-slate-12 transition-colors text-left"
                onClick={() => {
                  document.documentElement.classList.toggle("dark");
                  setOpen(false);
                }}
              >
                <Palette className="size-4 shrink-0" />
                <span>Change appearance</span>
              </button>
            </li>
            <li className="my-1 border-t border-n-weak" />
            <li>
              <button
                type="button"
                className="flex items-center gap-2.5 w-full px-2 py-1.5 rounded-lg text-sm text-n-ruby-11 hover:bg-n-ruby-3 transition-colors text-left font-medium"
                onClick={handleLogout}
              >
                <LogOut className="size-4 shrink-0" />
                <span>Log out</span>
              </button>
            </li>
          </ul>
        </div>
      )}
    </div>
  );
}
