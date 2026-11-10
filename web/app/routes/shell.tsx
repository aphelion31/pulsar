// Authenticated app shell: sidebar + realtime connection. Children read context via useOutletContext<ShellCtx>().
import { useEffect, useRef, useState } from "react";
import { NavLink, Outlet, redirect, useLoaderData, useNavigate, useParams } from "react-router";
import { Contact2, Inbox, LogOut, Settings } from "lucide-react";
import { Avatar } from "~/components/ui";
import { api, getSession, signOut, type Profile } from "~/lib/api";
import { connectCable, type CableEvent } from "~/lib/cable";

export async function clientLoader() {
  if (!getSession()) throw redirect("/login");
  const me = await api<Profile>("/api/v1/profile");
  return { me };
}

export type ShellCtx = { me: Profile; accountId: number; subscribe: (fn: (e: CableEvent) => void) => () => void };

export default function Shell() {
  const { me } = useLoaderData<typeof clientLoader>();
  const params = useParams();
  const nav = useNavigate();
  const accountId = Number(params.accountId ?? me.account_id);
  const account = me.accounts.find((a) => a.id === accountId) ?? me.accounts[0];
  const listeners = useRef(new Set<(e: CableEvent) => void>());
  const [online, setOnline] = useState(false);

  useEffect(() => {
    const stop = connectCable({ pubsubToken: me.pubsub_token, accountId, userId: me.id }, (e) => {
      if (e.event === "presence.update") setOnline(true);
      listeners.current.forEach((fn) => fn(e));
    });
    return stop;
  }, [me.pubsub_token, me.id, accountId]);

  const ctx: ShellCtx = {
    me,
    accountId,
    subscribe: (fn) => {
      listeners.current.add(fn);
      return () => listeners.current.delete(fn);
    },
  };

  const item = "flex h-9 items-center gap-2.5 rounded-lg px-2.5 text-[13px] transition-colors duration-150";
  const cls = ({ isActive }: { isActive: boolean }) =>
    `${item} ${isActive ? "bg-surface text-ink shadow-[0_0_0_1px_var(--color-line)]" : "text-ink-2 hover:bg-raised hover:text-ink"}`;

  return (
    <div className="flex h-dvh">
      <aside className="flex w-60 shrink-0 flex-col border-r border-line bg-bg px-3 py-4">
        <div className="flex items-center justify-between px-2.5">
          <span className="text-[15px] font-semibold tracking-tight">Pulsar</span>
          <span
            className={`size-2 rounded-full ${online ? "bg-ok" : "bg-ink-3"}`}
            title={online ? "Live updates connected" : "Connecting"}
            aria-label={online ? "Live updates connected" : "Connecting"}
          />
        </div>
        <p className="mt-1 truncate px-2.5 text-[12px] text-ink-3">{account?.name}</p>
        <nav className="mt-6 grid gap-0.5" aria-label="Main">
          <NavLink to={`/accounts/${accountId}/conversations`} className={cls}>
            <Inbox size={16} strokeWidth={1.5} /> Conversations
          </NavLink>
          <NavLink to={`/accounts/${accountId}/contacts`} className={cls}>
            <Contact2 size={16} strokeWidth={1.5} /> Contacts
          </NavLink>
          <NavLink to={`/accounts/${accountId}/settings`} className={cls}>
            <Settings size={16} strokeWidth={1.5} /> Settings
          </NavLink>
        </nav>
        <div className="mt-auto flex items-center gap-2.5 border-t border-line px-1 pt-4">
          <Avatar name={me.available_name} url={me.avatar_url || undefined} size={28} />
          <div className="min-w-0 flex-1">
            <p className="truncate text-[13px] font-medium">{me.available_name}</p>
            <p className="truncate text-[12px] text-ink-3">{me.email}</p>
          </div>
          <button
            type="button"
            aria-label="Sign out"
            className="grid size-8 place-items-center rounded-lg text-ink-3 transition-colors duration-150 hover:bg-raised hover:text-ink"
            onClick={async () => {
              await signOut();
              nav("/login", { replace: true });
            }}
          >
            <LogOut size={16} strokeWidth={1.5} />
          </button>
        </div>
      </aside>
      <main className="flex min-w-0 flex-1 flex-col bg-surface">
        <Outlet context={ctx} />
      </main>
    </div>
  );
}
