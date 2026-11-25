// Authenticated app shell: sidebar + realtime connection + command bar. Children read context via useOutletContext<ShellCtx>().
import { Menu } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Outlet, redirect, useLoaderData, useParams } from "react-router";
import { CommandBar } from "~/components/next/CommandBar";
import { KeyboardShortcutsModal } from "~/components/next/KeyboardShortcutsModal";
import { Sidebar } from "~/components/next/sidebar";
import { api, getSession, type Profile } from "~/lib/api";
import { connectCable, type CableEvent } from "~/lib/cable";

export async function clientLoader() {
  if (!getSession()) throw redirect("/login");
  const me = await api<Profile>("/api/v1/profile");
  return { me };
}

export type ShellCtx = {
  me: Profile;
  accountId: number;
  subscribe: (fn: (e: CableEvent) => void) => () => void;
};

export default function Shell() {
  const { me } = useLoaderData<typeof clientLoader>();
  const params = useParams();
  const accountId = Number(params.accountId ?? me.account_id);
  const listeners = useRef(new Set<(e: CableEvent) => void>());
  const [online, setOnline] = useState(false);
  const [mobileOpen, setMobileOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);

  useEffect(() => {
    const stop = connectCable({ pubsubToken: me.pubsub_token, accountId, userId: me.id }, (e) => {
      if (e.event === "presence.update") setOnline(true);
      listeners.current.forEach((fn) => fn(e));
    });
    return stop;
  }, [me.pubsub_token, me.id, accountId]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const inField =
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT" ||
          target.isContentEditable);
      if (inField) return;
      if (e.key === "?") {
        e.preventDefault();
        setShortcutsOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const subscribe = useCallback((fn: (e: CableEvent) => void) => {
    listeners.current.add(fn);
    return () => {
      listeners.current.delete(fn);
    };
  }, []);

  const ctx: ShellCtx = useMemo(() => ({ me, accountId, subscribe }), [me, accountId, subscribe]);

  return (
    <div className="flex h-dvh bg-n-background text-n-slate-12">
      <Sidebar
        me={me}
        accountId={accountId}
        online={online}
        mobileOpen={mobileOpen}
        onCloseMobile={() => setMobileOpen(false)}
        onOpenShortcuts={() => setShortcutsOpen(true)}
      />
      <main className="relative flex min-w-0 flex-1 flex-col bg-n-surface-1">
        <button
          type="button"
          onClick={() => setMobileOpen(true)}
          aria-label="Open navigation menu"
          className="fixed bottom-4 left-4 z-30 flex size-10 items-center justify-center rounded-full border border-n-weak bg-n-solid-3 text-n-slate-12 shadow-lg transition hover:brightness-105 md:hidden"
        >
          <Menu className="size-5" />
        </button>
        <Outlet context={ctx} />
      </main>
      <CommandBar
        me={me}
        accountId={accountId}
        onOpenShortcuts={() => setShortcutsOpen(true)}
      />
      <KeyboardShortcutsModal
        open={shortcutsOpen}
        onClose={() => setShortcutsOpen(false)}
      />
    </div>
  );
}
