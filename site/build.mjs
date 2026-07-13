import * as esbuild from "esbuild";
import { readFileSync, writeFileSync, mkdirSync, existsSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const key = process.env.POSTHOG_PROJECT_KEY || "";
const host = process.env.POSTHOG_HOST || "https://us.i.posthog.com";

if (!key) {
  console.warn(
    "[site] POSTHOG_PROJECT_KEY unset — analytics bundle will be a no-op stub",
  );
}

mkdirSync(join(__dirname, "dist"), { recursive: true });

const define = {
  "process.env.POSTHOG_PROJECT_KEY": JSON.stringify(key),
  "process.env.POSTHOG_HOST": JSON.stringify(host),
};

if (key) {
  await esbuild.build({
    entryPoints: [join(__dirname, "analytics.js")],
    bundle: true,
    minify: true,
    outfile: join(__dirname, "dist/analytics.js"),
    format: "iife",
    define,
    // No CDN — fully bundled.
  });
} else {
  writeFileSync(join(__dirname, "dist/analytics.js"), "/* posthog inert */\n");
}

// Prefer a hand-authored index.html; keep analytics script tag if present.
const srcHtml = join(__dirname, "index.html");
let html = existsSync(srcHtml)
  ? readFileSync(srcHtml, "utf8")
  : `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Hypervisor</title>
</head>
<body>
  <p>Hypervisor — site content lands with DEPLOY.</p>
  <a href="/releases/latest/" data-download>Download</a>
  <script src="./analytics.js"></script>
</body>
</html>
`;

if (!html.includes("analytics.js")) {
  html = html.replace(
    "</body>",
    '  <script src="./analytics.js"></script>\n</body>',
  );
}

writeFileSync(join(__dirname, "dist/index.html"), html);
console.log("[site] built → site/dist/");
