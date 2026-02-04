// overlay/live-reload.js
let lastCssHash = null;

async function checkCss() {
  try {
    const r = await fetch('/style.css', { cache: 'no-store' });
    if (!r.ok) return;
    const text = await r.text();

    // Hash to detect changes
    const buf = new TextEncoder().encode(text);
    const digest = await crypto.subtle.digest('SHA-1', buf);
    const hash = Array.from(new Uint8Array(digest))
      .map(b => b.toString(16).padStart(2, '0'))
      .join('');

    if (hash !== lastCssHash) {
      lastCssHash = hash;
      const link = document.querySelector('link[rel="stylesheet"]');
      // cache-bust to force reload without page refresh
      const base = link.getAttribute('data-base') || '/style.css';
      link.href = base + (base.includes('?') ? '&' : '?') + 't=' + Date.now();
    }
  } catch (_) {}
}

// Kick off polling
checkCss();
setInterval(checkCss, 300);
