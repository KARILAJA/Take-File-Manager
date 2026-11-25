// Intercept left-clicks on magnet: links and forward them to TurboGet Extension background worker
document.addEventListener(
  "click",
  (event) => {
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {
      return;
    }
    const anchor = event.target && event.target.closest ? event.target.closest("a[href]") : null;
    if (!anchor) return;
    const href = anchor.getAttribute("href") || "";
    if (!href.trim().toLowerCase().startsWith("magnet:")) return;

    event.preventDefault();
    event.stopPropagation();

    chrome.runtime.sendMessage(
      {
        type: "magnet_link_clicked",
        url: href.trim(),
        referrer: window.location.href
      },
      (resp) => {
        if (chrome.runtime.lastError || !resp || !resp.ok) {
          // Fallback to browser default handler if TurboGet Core is unreachable
          window.location.assign(href.trim());
        }
      }
    );
  },
  true
);
