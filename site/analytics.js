import posthog from "posthog-js";

// Injected at bundle time by site/build.mjs (production PostHog project).
const key = process.env.POSTHOG_PROJECT_KEY || "";
const host = process.env.POSTHOG_HOST || "https://us.i.posthog.com";

if (key && !(navigator.doNotTrack === "1" || window.doNotTrack === "1")) {
  posthog.init(key, {
    api_host: host,
    persistence: "memory",
    autocapture: false,
    capture_pageview: true,
    disable_session_recording: true,
    person_profiles: "never",
  });

  document.addEventListener("click", (e) => {
    const t = e.target;
    if (!(t instanceof Element)) return;
    const a = t.closest("a[data-download], a[href*='releases'], a.download-btn");
    if (a) {
      posthog.capture("site_download_click");
    }
  });
}
