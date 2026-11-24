#!/usr/bin/env node

// Check the links between pages of the docs.
// scripts/lib/check_docs_links.mjs docs/dist

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { unescape } from "node:querystring";

const BASE = "/";
const dist = process.argv[2];

if (!dist || !existsSync(dist)) {
  console.error(
    "usage: scripts/lib/check_docs_links.mjs <built site directory>",
  );
  process.exit(2);
}

const pages = [];
const walk = (dir) => {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) walk(path);
    else if (name.endsWith(".html")) pages.push(path);
  }
};

walk(dist);

const html = new Map();
const read = (file) => {
  if (!html.has(file)) html.set(file, readFileSync(file, "utf8"));
  return html.get(file);
};

const hasId = (text, id) => text.includes(`id="${id}"`);
const unescape = (s) =>
  s.replaceAll("&amp;", "&").replaceAll("&quot;", '"').replaceAll("&#39;", "'");

const broken = [];
for (const page of pages) {
  const text = read(page);
  for (const [, raw] of text.matchAll(/href="([^"]+)"/g)) {
    const href = unescape(raw);
    if (href.startsWith("#")) {
      if (
        href !== "#_top" &&
        href.length > 1 &&
        !hasId(text, decodeURIComponent(href.slice(1)))
      ) {
        broken.push([page, href]);
      }
      continue;
    }
    if (!href.startsWith("/") || href.startsWith("//")) continue;
    if (!href.startsWith(BASE)) {
      broken.push([page, `${href} (missing the ${BASE} base)`]);
      continue;
    }
    const [path, anchor] = href.slice(BASE.length).split("#");
    let target = join(dist, decodeURIComponent(path));
    if (existsSync(target) && statSync(target).isDirectory())
      target = join(target, "index.html");
    if (!existsSync(target)) broken.push([page, href]);
    else if (
      anchor &&
      target.endsWith(".html") &&
      !hasId(read(target), decodeURIComponent(anchor))
    ) {
      broken.push([page, href]);
    }
  }
}

for (const [page, href] of broken) console.log(`${page}: ${href}`);
if (broken.length) {
  console.log(`${broken.length} broken link(s) in ${pages.length} pages`);
  process.exit(1);
}
console.log(`${pages.length} pages, links ok`);
