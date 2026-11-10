// Minimal ActionCable client for Pulsar's /cable (actioncable-v1-json protocol).
export type CableEvent = { event: string; data: any };

export function connectCable(params: { pubsubToken: string; accountId: number; userId: number }, onEvent: (e: CableEvent) => void) {
  const identifier = JSON.stringify({
    channel: "RoomChannel",
    pubsub_token: params.pubsubToken,
    account_id: params.accountId,
    user_id: params.userId,
  });
  let ws: WebSocket | null = null;
  let closed = false;
  let presence: ReturnType<typeof setInterval> | undefined;
  let retry: ReturnType<typeof setTimeout> | undefined;

  const open = () => {
    const proto = location.protocol === "https:" ? "wss" : "ws";
    ws = new WebSocket(`${proto}://${location.host}/cable`, ["actioncable-v1-json", "actioncable-unsupported"]);
    ws.onopen = () => ws?.send(JSON.stringify({ command: "subscribe", identifier }));
    ws.onmessage = (m) => {
      const msg = JSON.parse(m.data);
      if (msg.type === "confirm_subscription") {
        const ping = () => ws?.send(JSON.stringify({ command: "message", identifier, data: JSON.stringify({ action: "update_presence" }) }));
        ping();
        presence = setInterval(ping, 20000);
      } else if (msg.message?.event) {
        onEvent(msg.message as CableEvent);
      }
    };
    ws.onclose = () => {
      clearInterval(presence);
      if (!closed) retry = setTimeout(open, 2000);
    };
  };
  open();
  return () => {
    closed = true;
    clearInterval(presence);
    clearTimeout(retry);
    ws?.close();
  };
}
