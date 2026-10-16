#!/usr/bin/env node
"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { pathToFileURL } = require("node:url");

const executablePath = process.env.CUTOUT_BROWSER_EXECUTABLE;
if (!executablePath) {
  throw new Error("Set CUTOUT_BROWSER_EXECUTABLE to the installed Chrome/Chromium executable.");
}
const playwrightPath = process.env.PLAYWRIGHT_MODULE || "playwright";
const { chromium } = require(playwrightPath);
const sourcePath = path.join(__dirname, "index.html");
const outputDirectory = path.resolve(__dirname, "..");
const scenes = ["rgb", "tune", "ride-tracking"];

async function main() {
  if (!fs.existsSync(executablePath)) {
    throw new Error(`Browser executable does not exist: ${executablePath}`);
  }

  for (const scene of scenes) {
    let browser;
    try {
      browser = await chromium.launch({ executablePath, headless: true, args: ["--disable-gpu"] });
      const context = await browser.newContext({
        viewport: { width: 430, height: 932 },
        deviceScaleFactor: 3,
        isMobile: true,
        hasTouch: true,
      });
      try {
        const page = await context.newPage();
        const url = `${pathToFileURL(sourcePath).href}?scene=${encodeURIComponent(scene)}`;
        await page.goto(url, { waitUntil: "load" });
        await page.evaluate(async () => {
          await document.fonts.ready;
          await Promise.all(Array.from(document.images, (image) => image.decode()));
          await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
        });
        const outputPath = path.join(outputDirectory, `${scene}.png`);
        await page.screenshot({ path: outputPath, fullPage: false, type: "png" });
        process.stdout.write(`${scene}: ${path.relative(process.cwd(), outputPath)}\n`);
      } finally {
        await context.close();
      }
    } finally {
      if (browser) await browser.close();
    }
  }
}

main().catch((error) => {
  process.stderr.write(`${error.stack || error}\n`);
  process.exitCode = 1;
});
