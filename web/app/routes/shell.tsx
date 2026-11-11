// Authenticated app shell: sidebar + realtime connection. Children read context via useOutletContext<ShellCtx>().
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Outlet, redirect, useLoaderData, useParams } from "react-router";
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

  useEffect(() => {
    const stop = connectCable({ pubsubToken: me.pubsub_token, accountId, userId: me.id }, (e) => {
      if (e.event === "presence.update") setOnline(true);
      listeners.current.forEach((fn) => fn(e));
    });
    return stop;
  }, [me.pubsub_token, me.id, accountId]);

  const subscribe = useCallback((fn: (e: CableEvent) => void) => {
    listeners.current.add(fn);
    return () => {
      listeners.current.delete(fn);
    };
  }, []);

  const ctx: ShellCtx = useMemo(() => ({ me, accountId, subscribe }), [me, accountId, subscribe]);

  return (
    <div className="flex h-dvh">
      <Sidebar me={me} accountId={accountId} online={online} />
      <main className="flex min-w-0 flex-1 flex-col bg-surface">
        <Outlet context={ctx} />
      </main>
    </div>
  );
}
