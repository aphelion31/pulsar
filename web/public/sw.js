// Pulsar Service Worker for browser Web Push notifications (RFC 8291 / Push API).
self.addEventListener("push", (event) => {
  let data = {};
  if (event.data) {
    try {
      data = event.data.json();
    } catch {
      data = { body: event.data.text() };
    }
  }
  const title = data.title || "Pulsar";
  const body = data.body || "New conversation activity";
  const url = data.url || "/app/";
  event.waitUntil(
    self.registration.showNotification(title, {
      body,
      icon: "/app/brand-assets/logo_thumbnail.svg",
      badge: "/app/brand-assets/logo_thumbnail.svg",
      data: { url, account_id: data.account_id, conversation_id: data.conversation_id },
    }),
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const targetUrl = (event.notification.data && event.notification.data.url) || "/app/";
  event.waitUntil(
    self.clients.matchAll({ type: "window", includeUncontrolled: true }).then((clients) => {
      for (const client of clients) {
        if ("focus" in client) {
          client.navigate(targetUrl).catch(() => {});
          return client.focus();
        }
      }
      if (self.clients.openWindow) {
        return self.clients.openWindow(targetUrl);
      }
      return undefined;
    }),
  );
});
