// Thank-you page: trigger the actual download, then render platform-
// specific first-run instructions. Mobile visitors see the same friendly
// "open on your computer" message as the landing page.

const RELEASES_PAGE = "https://github.com/myketheguru/travis-releases/releases";

const PLATFORM_TITLES = {
  windows: "Windows",
  macos: "macOS",
  linux: "Linux",
};

// Per-platform first-run guidance. Same shape — heading + steps array.
const INSTRUCTIONS = {
  windows: {
    heading: "Windows",
    steps: [
      'Open the downloaded <code>.exe</code>. SmartScreen will say <em>"Windows protected your PC"</em>.',
      'Click <strong>More info</strong>, then <strong>Run anyway</strong>. This is normal for unsigned apps during preview — the binary is signed by Travis\'s updater key, just not (yet) by a Windows code-signing certificate.',
      'Follow the installer. Travis launches automatically when it finishes.',
      "Press <kbd>Ctrl</kbd> + <kbd>J</kbd> anywhere to open the capture overlay.",
    ],
  },
  macos: {
    heading: "macOS (Intel + Apple Silicon)",
    steps: [
      'Open the downloaded <code>.dmg</code> and drag <strong>Travis.app</strong> into <strong>Applications</strong>.',
      'On first launch, macOS will block the app: <em>"Travis cannot be opened because it is from an unidentified developer."</em>',
      'Open Finder → Applications, <strong>right-click Travis</strong>, choose <strong>Open</strong>, then <strong>Open</strong> in the dialog. You only do this once.',
      "Press <kbd>⌘</kbd> + <kbd>J</kbd> anywhere to open the capture overlay.",
    ],
  },
  linux: {
    heading: "Linux",
    steps: [
      'For the AppImage: <code>chmod +x Travis_*.AppImage</code> then run it directly.',
      'For the .deb (Debian / Ubuntu): <code>sudo apt install ./Travis_*.deb</code> — same for the .rpm with <code>dnf install</code>.',
      "Press <kbd>Ctrl</kbd> + <kbd>J</kbd> anywhere to open the capture overlay.",
    ],
  },
};

function getParams() {
  const p = new URLSearchParams(window.location.search);
  return {
    platform: p.get("platform"),
    url: p.get("url"),
  };
}

function applyMobileMode() {
  document.body.classList.add("is-mobile");
  // Hide the desktop-only sections.
  for (const id of ["thanks-instructions", "thanks-next"]) {
    const el = document.getElementById(id);
    if (el) el.setAttribute("hidden", "");
  }
  const fallback = document.getElementById("thanks-fallback");
  if (fallback) fallback.setAttribute("hidden", "");
  const status = document.getElementById("thanks-status");
  if (status) status.setAttribute("hidden", "");

  const notice = document.getElementById("mobile-notice");
  if (notice) notice.removeAttribute("hidden");

  const copyBtn = document.getElementById("copy-link");
  const copyStatus = document.getElementById("copy-link-status");
  if (copyBtn) {
    copyBtn.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(window.location.href);
        if (copyStatus) {
          copyStatus.textContent =
            "Copied. Email it to yourself or open it on your computer.";
        }
      } catch {
        if (copyStatus) {
          copyStatus.textContent =
            "Couldn't copy automatically — long-press the URL bar to copy.";
        }
      }
    });
  }
}

function startDownload(url) {
  if (!url) return;
  // Use a hidden anchor + click to start the download. Setting
  // window.location.href would also work since GitHub release assets send
  // Content-Disposition: attachment, but the anchor approach is more
  // bulletproof across browsers (Safari especially).
  const a = document.createElement("a");
  a.href = url;
  a.rel = "noopener";
  // The browser will follow the redirect chain to the actual binary.
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
}

function renderInstructions(platform) {
  const container = document.getElementById("platform-instructions");
  if (!container) return;
  const spec = INSTRUCTIONS[platform];
  if (!spec) {
    container.innerHTML =
      '<p class="instruction-fallback">' +
      'Pick the right installer for your OS on the ' +
      `<a href="${RELEASES_PAGE}">releases page</a>.</p>`;
    return;
  }
  const stepsHtml = spec.steps
    .map((s, i) => `<li><span class="step-num">${i + 1}</span><div>${s}</div></li>`)
    .join("");
  container.innerHTML =
    `<h3>${spec.heading}</h3>` + `<ol class="thanks-steps">${stepsHtml}</ol>`;
}

function setManualLink(url) {
  const a = document.getElementById("manual-link");
  if (!a) return;
  if (url) {
    a.setAttribute("href", url);
  } else {
    a.setAttribute("href", RELEASES_PAGE);
    a.textContent = "Open the releases page";
  }
}

function setStatus(platform) {
  const status = document.getElementById("thanks-status");
  if (!status) return;
  const title = PLATFORM_TITLES[platform];
  status.textContent = title
    ? `Your ${title} download should start in a moment.`
    : "Your download should start in a moment.";
}

function init() {
  if (window.travis && window.travis.isMobile && window.travis.isMobile()) {
    applyMobileMode();
    return;
  }

  const { platform, url } = getParams();
  setStatus(platform);
  setManualLink(url);
  renderInstructions(platform);

  // Kick off the actual download a beat after paint so the page can
  // render before the browser shifts focus to its download chrome.
  if (url) {
    setTimeout(() => startDownload(url), 350);
  }
}

init();
