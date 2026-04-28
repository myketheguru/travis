// Shared helpers used by both index.html and thanks.html. Loaded BEFORE
// the page-specific script so we can short-circuit mobile rendering
// before any flash of desktop-only content.

window.travis = window.travis || {};

window.travis.isMobile = function isMobile() {
  // 1) Modern hint (Chrome/Edge expose this; Safari does not).
  if (navigator.userAgentData && navigator.userAgentData.mobile) return true;

  // 2) iPad on iPadOS 13+ reports as Macintosh in userAgent — disambiguate
  //    via touch points.
  const ua = (navigator.userAgent || "").toLowerCase();
  const isMacWithTouch =
    /macintosh/.test(ua) && navigator.maxTouchPoints && navigator.maxTouchPoints > 1;
  if (isMacWithTouch) return true;

  // 3) Plain mobile UA strings.
  if (/(android|iphone|ipad|ipod|blackberry|webos|opera mini|iemobile|mobile)/.test(ua))
    return true;

  // 4) Coarse pointer + small viewport — catches most edge cases.
  if (
    window.matchMedia &&
    window.matchMedia("(pointer: coarse)").matches &&
    window.innerWidth < 820
  ) {
    return true;
  }

  return false;
};

// SVG markup for the primary CTA's platform icon. Indexed by the same
// platform keys used everywhere else (windows / macos / linux). The
// path data here is the same one used by the download cards in index.html
// — keep them in sync if you update either.
window.travis.platformIcons = {
  windows: `<svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor" aria-hidden="true">
    <path d="M3 5.5l7.5-1v8H3v-7zM11 4.4L21 3v9.5h-10V4.4zM3 13.5h7.5v8L3 20.5v-7zM11 13.5h10V21l-10-1.5v-6z"/>
  </svg>`,
  macos: `<svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor" aria-hidden="true">
    <path d="M16.4 11.5c0-2.4 2-3.6 2-3.6-1.1-1.6-2.8-1.8-3.4-1.8-1.5-.1-2.8.8-3.5.8s-1.9-.8-3.1-.8c-1.6 0-3.1.9-3.9 2.4-1.7 2.9-.4 7.1 1.2 9.4.8 1.1 1.7 2.4 3 2.3 1.2 0 1.6-.8 3.1-.8s1.8.8 3.1.8c1.3 0 2.1-1.1 2.9-2.3.9-1.3 1.3-2.6 1.3-2.6s-2.7-1.1-2.7-3.8zM14.2 4.6c.7-.8 1.1-1.9 1-3-.9.1-2 .6-2.7 1.4-.6.7-1.2 1.8-1 2.9 1 .1 2-.5 2.7-1.3z"/>
  </svg>`,
  linux: `<svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor" aria-hidden="true">
    <path d="M12 2c-3.3 0-6 2.7-6 6 0 1.5.6 2.9 1.5 4-1.6 1.4-2.5 3.4-2.5 5.5 0 1.5.5 2.9 1.4 4 .9 1.1 2.1 1.8 3.4 1.9.4 0 .7-.2.9-.5l1.3-2.4 1.3 2.4c.2.3.5.5.9.5 1.3-.1 2.5-.8 3.4-1.9.9-1.1 1.4-2.5 1.4-4 0-2.1-.9-4.1-2.5-5.5.9-1.1 1.5-2.5 1.5-4 0-3.3-2.7-6-6-6zm-2 7c-.6 0-1-.4-1-1s.4-1 1-1 1 .4 1 1-.4 1-1 1zm4 0c-.6 0-1-.4-1-1s.4-1 1-1 1 .4 1 1-.4 1-1 1z"/>
  </svg>`,
};
