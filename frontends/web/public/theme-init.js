(() => {
  try {
    const mode = window.localStorage.getItem("agentics-theme") || "system";
    const prefersDark = window.matchMedia(
      "(prefers-color-scheme: dark)",
    ).matches;
    const theme = mode === "system" ? (prefersDark ? "dark" : "light") : mode;
    document.documentElement.dataset.theme = theme;
  } catch {
    document.documentElement.dataset.theme = "dark";
  }

  try {
    const ua = navigator.userAgent;
    const vendor = navigator.vendor || "";
    const isSafari =
      vendor.includes("Apple") &&
      /Safari/.test(ua) &&
      !/Chrome|Chromium|CriOS|FxiOS|Edg|OPR/.test(ua);
    const isChromium =
      vendor.includes("Google") || /Chrome|Chromium|CriOS|Edg|OPR/.test(ua);

    document.documentElement.dataset.browserEngine = isSafari
      ? "safari"
      : isChromium
        ? "chromium"
        : "other";
  } catch {
    document.documentElement.dataset.browserEngine = "other";
  }
})();
