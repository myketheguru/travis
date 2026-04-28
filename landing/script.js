// OS detection + per-platform download wiring.
//
// We pull the latest GitHub release JSON, find the asset matching each
// platform's extension, and set the buttons' hrefs to direct
// browser_download_url values. This avoids hardcoding versions.
//
// If the API call fails (rate-limited, no release yet, etc.) we fall
// back to the public releases page. The primary CTA also falls back to
// the all-platforms #download anchor.

const RELEASES_REPO = "myketheguru/travis-releases";
const RELEASES_API = `https://api.github.com/repos/${RELEASES_REPO}/releases/latest`;
const RELEASES_PAGE = `https://github.com/${RELEASES_REPO}/releases`;

const PLATFORM_LABELS = {
  windows: "for Windows",
  macos: "for macOS",
  linux: "for Linux",
};

// Map each platform to:
//   exts:  preferred file-extension matchers (first match wins)
//   label: friendly format label shown under the platform name
const PLATFORMS = {
  windows: {
    exts: [
      { test: (n) => n.endsWith("-setup.exe"), label: ".exe (NSIS)" },
      { test: (n) => n.endsWith(".msi"),       label: ".msi" },
    ],
  },
  macos: {
    exts: [
      { test: (n) => n.endsWith("_universal.dmg"),                          label: ".dmg (universal)" },
      { test: (n) => n.endsWith(".dmg"),                                     label: ".dmg" },
      { test: (n) => n.endsWith(".app.tar.gz") && !n.includes(".sig"),      label: ".app.tar.gz" },
    ],
  },
  linux: {
    exts: [
      { test: (n) => n.endsWith(".AppImage"), label: ".AppImage (x64)" },
      { test: (n) => n.endsWith(".deb"),       label: ".deb" },
    ],
  },
};

function detectOS() {
  const ua = (navigator.userAgent || "").toLowerCase();
  const platform = (navigator.platform || "").toLowerCase();
  if (platform.includes("win") || ua.includes("windows")) return "windows";
  if (platform.includes("mac") || ua.includes("mac os") || ua.includes("macintosh")) return "macos";
  if (ua.includes("linux") || ua.includes("x11")) return "linux";
  return null;
}

function findAsset(assets, platformKey) {
  const spec = PLATFORMS[platformKey];
  if (!spec) return null;
  for (const matcher of spec.exts) {
    const hit = assets.find((a) => matcher.test(a.name.toLowerCase()));
    if (hit) {
      return { asset: hit, label: matcher.label };
    }
  }
  return null;
}

function setPrimaryCta({ os, label, href }) {
  const cta = document.getElementById("dl-primary");
  const platformLabel = document.getElementById("dl-primary-platform");
  const iconSlot = document.getElementById("dl-primary-icon");
  if (!cta || !platformLabel) return;
  if (!os || !href) {
    platformLabel.textContent = "All platforms";
    cta.setAttribute("href", "#download");
    if (iconSlot) iconSlot.innerHTML = "";
    return;
  }
  platformLabel.textContent = `${label} ${PLATFORM_LABELS[os] || ""}`.trim();
  cta.setAttribute("href", href);
  if (iconSlot && window.travis && window.travis.platformIcons) {
    iconSlot.innerHTML = window.travis.platformIcons[os] || "";
  }
}

function thanksUrl(platformKey, assetUrl) {
  const params = new URLSearchParams({ platform: platformKey, url: assetUrl });
  return `thanks.html?${params.toString()}`;
}

function setCard(platformKey, { href, formatLabel }) {
  const card = document.querySelector(`.dl-card[data-platform="${platformKey}"]`);
  if (!card) return;
  if (!href) {
    card.setAttribute("aria-disabled", "true");
    card.removeAttribute("href");
    const slot = card.querySelector('[data-slot="format"]');
    if (slot) slot.textContent = "Not available yet";
    return;
  }
  // Hand off through the thanks page so the user gets first-run guidance.
  card.setAttribute("href", thanksUrl(platformKey, href));
  const slot = card.querySelector('[data-slot="format"]');
  if (slot && formatLabel) slot.textContent = formatLabel;
}

function setVersionLabel(tagName) {
  const el = document.getElementById("version-label");
  if (!el) return;
  if (tagName) el.textContent = tagName;
}

function fallback(detectedOs) {
  // No release info; point everything at the releases page.
  setPrimaryCta({
    os: detectedOs,
    label: "Download",
    href: RELEASES_PAGE,
  });
  for (const platformKey of Object.keys(PLATFORMS)) {
    setCard(platformKey, { href: RELEASES_PAGE });
  }
}

function applyMobileMode() {
  document.body.classList.add("is-mobile");
  // Hide everything that assumes a downloadable artifact.
  const hide = ["downloads-primary", "download"];
  for (const id of hide) {
    const el = document.getElementById(id);
    if (el) el.setAttribute("hidden", "");
  }
  const meta = document.querySelector(".meta");
  if (meta) meta.setAttribute("hidden", "");

  // Reveal the friendly mobile message + wire the copy-link button.
  const notice = document.getElementById("mobile-notice");
  if (notice) notice.removeAttribute("hidden");
  const copyBtn = document.getElementById("copy-link");
  const status = document.getElementById("copy-link-status");
  if (copyBtn) {
    copyBtn.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(window.location.href);
        if (status) status.textContent = "Copied. Email it to yourself or open it on your computer.";
      } catch {
        if (status) status.textContent = "Couldn't copy automatically — long-press the URL bar to copy.";
      }
    });
  }
}

async function init() {
  if (window.travis && window.travis.isMobile && window.travis.isMobile()) {
    applyMobileMode();
    return;
  }

  const detectedOs = detectOS();

  // Optimistic placeholder: show "Download for X" while we fetch.
  if (detectedOs) {
    setPrimaryCta({
      os: detectedOs,
      label: "Download",
      href: "#download",
    });
  } else {
    setPrimaryCta({ os: null });
  }

  let release;
  try {
    const resp = await fetch(RELEASES_API, {
      headers: { Accept: "application/vnd.github+json" },
    });
    if (!resp.ok) throw new Error(`releases ${resp.status}`);
    release = await resp.json();
  } catch (e) {
    console.warn("Couldn't fetch latest release:", e);
    fallback(detectedOs);
    return;
  }

  if (!release || !Array.isArray(release.assets) || release.assets.length === 0) {
    fallback(detectedOs);
    return;
  }

  setVersionLabel(release.tag_name || "latest");

  // Wire each download card.
  const matched = {};
  for (const platformKey of Object.keys(PLATFORMS)) {
    const found = findAsset(release.assets, platformKey);
    if (found) {
      matched[platformKey] = found;
      setCard(platformKey, {
        href: found.asset.browser_download_url,
        formatLabel: found.label,
      });
    } else {
      setCard(platformKey, { href: null });
    }
  }

  // Wire primary CTA — route through the thanks page so the user gets
  // first-run guidance alongside the download.
  if (detectedOs && matched[detectedOs]) {
    setPrimaryCta({
      os: detectedOs,
      label: "Download",
      href: thanksUrl(detectedOs, matched[detectedOs].asset.browser_download_url),
    });
  } else if (detectedOs) {
    // Detected OS, but no matching asset — point to the releases page.
    setPrimaryCta({
      os: detectedOs,
      label: "Download",
      href: RELEASES_PAGE,
    });
  } else {
    setPrimaryCta({ os: null });
  }

  // Releases-page link in the fine print.
  const link = document.getElementById("releases-link");
  if (link && release.html_url) link.setAttribute("href", release.html_url);
}

init();
