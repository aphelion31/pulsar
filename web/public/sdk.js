/* Pulsar website chat loader. Embed:
   <script src="https://YOUR-PULSAR/sdk.js" data-website-token="TOKEN" async></script>
   Adds a launcher button and opens /widget in an iframe. */
(function () {
  var s = document.currentScript;
  var token = s && s.getAttribute("data-website-token");
  if (!token || window.__pulsarWidget) return;
  window.__pulsarWidget = true;
  var base = new URL(s.src).origin;
  var color = s.getAttribute("data-color") || "#1f6fd1";

  var frame = document.createElement("iframe");
  frame.src = base + "/widget?website_token=" + encodeURIComponent(token);
  frame.title = "Chat";
  frame.style.cssText =
    "position:fixed;right:20px;bottom:88px;width:380px;height:600px;max-height:calc(100vh - 108px);max-width:calc(100vw - 40px);" +
    "border:0;border-radius:12px;box-shadow:0 8px 32px rgba(0,0,0,.16);z-index:2147483000;display:none;background:#fff";

  var btn = document.createElement("button");
  btn.type = "button";
  btn.setAttribute("aria-label", "Open chat");
  btn.setAttribute("aria-expanded", "false");
  btn.style.cssText =
    "position:fixed;right:20px;bottom:20px;width:56px;height:56px;border:0;border-radius:50%;cursor:pointer;z-index:2147483000;" +
    "background:" + color + ";color:#fff;box-shadow:0 4px 16px rgba(0,0,0,.18);display:grid;place-items:center";
  btn.innerHTML =
    '<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">' +
    '<path d="M21 12a8 8 0 0 1-11.6 7.1L4 20l1-4.6A8 8 0 1 1 21 12Z"/></svg>';
  btn.onclick = function () {
    var open = frame.style.display === "none";
    frame.style.display = open ? "block" : "none";
    btn.setAttribute("aria-expanded", String(open));
    btn.setAttribute("aria-label", open ? "Close chat" : "Open chat");
  };

  document.body.appendChild(frame);
  document.body.appendChild(btn);
})();
